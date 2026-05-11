import 'dart:ffi';
import 'dart:typed_data';

import 'package:ffi/ffi.dart';

import 'bindings.dart';
import 'error_codes.dart';
import 'ffi_structs.dart';
import 'types.dart';

export 'types.dart';

/// Interface principale de l'Engine de calendrier liturgique.
///
/// ## Cycle de vie
///
/// ```dart
/// final cal = LiturgicalCalendar();
/// cal.load(kaldBytes, litsBytes);   // copie en mémoire native, valide
///
/// final day = cal.getDay(2024, 12, 25);
/// print(day?.primary?.feastId);     // ID de la fête principale
///
/// final label = cal.getLabel(day!.primary!.feastId, 2024);
/// print(label?.label);              // "Nativitas Domini"
///
/// cal.dispose();                    // libère les buffers natifs
/// ```
///
/// ## Gestion mémoire
///
/// Les buffers natifs `.kald` et `.lits` sont alloués par [load] et libérés
/// par [dispose] (ou par le GC via [NativeFinalizer] si [dispose] n'est pas
/// appelé). Toutes les autres allocations sont temporaires et libérées à
/// l'intérieur de chaque appel via `using()`.
///
/// ## Thread safety
///
/// Les fonctions FFI du Core sont stateless — les appels concurrents sur la
/// même instance sont sûrs du côté Rust. La classe elle-même n'est pas
/// thread-safe : ne pas appeler [load]/[dispose] depuis plusieurs isolates.
class LiturgicalCalendar {
  Pointer<Uint8> _kaldPtr = nullptr;
  int            _kaldLen = 0;
  Pointer<Uint8> _litsPtr = nullptr;
  int            _litsLen = 0;
  bool           _loaded  = false;

  static final _kaldFinalizer = NativeFinalizer(calloc.nativeFree);
  static final _litsFinalizer = NativeFinalizer(calloc.nativeFree);

  final KalBindings _b;

  /// Construit un [LiturgicalCalendar] avec les bindings natifs par défaut.
  LiturgicalCalendar() : _b = KalBindings.instance;

  /// Construit avec des bindings injectés — pour les tests.
  LiturgicalCalendar.withBindings(KalBindings bindings) : _b = bindings;

  // ── Cycle de vie ────────────────────────────────────────────────────────────

  /// Charge les buffers `.kald` et `.lits` en mémoire native et les valide.
  ///
  /// **Validation effectuée :**
  /// 1. `kal_validate_header` : magic, version, schema, SHA-256 complet.
  /// 2. Cohérence `build_id` : `lits[12..20] == kald.checksum[0..8]`.
  ///
  /// Lève [KalValidationException] si le `.kald` est invalide.
  /// Lève [KalBuildIdMismatchException] si les deux fichiers ne correspondent pas.
  ///
  /// En cas d'exception, les buffers éventuellement alloués sont libérés
  /// avant le throw — l'instance est réutilisable.
  void load(Uint8List kaldData, Uint8List litsData) {
    _kaldLen = kaldData.length;
    _litsLen = litsData.length;

    // ── Allocation .kald ──────────────────────────────────────────────────────
    _kaldPtr = calloc<Uint8>(_kaldLen);
    _kaldPtr.asTypedList(_kaldLen).setAll(0, kaldData);
    _kaldFinalizer.attach(this, _kaldPtr.cast<Void>(), detach: this);

    // ── Allocation .lits ──────────────────────────────────────────────────────
    _litsPtr = calloc<Uint8>(_litsLen);
    _litsPtr.asTypedList(_litsLen).setAll(0, litsData);
    _litsFinalizer.attach(this, _litsPtr.cast<Void>(), detach: this);

    // ── Validation ────────────────────────────────────────────────────────────
    try {
      using((arena) {
        final headerPtr = arena<KalHeader>();

        // Validation SHA-256 complète.
        final rc = _b.kalValidateHeader(_kaldPtr, _kaldLen, headerPtr);
        if (rc != kalEngineOk) throw KalValidationException(rc);

        // Cohérence build_id : lits[12..20] doit égaler kald.checksum[0..8].
        if (_litsLen < 20) throw KalBuildIdMismatchException();
        for (int i = 0; i < 8; i++) {
          if (_litsPtr[12 + i] != headerPtr.ref.checksum[i]) {
            throw KalBuildIdMismatchException();
          }
        }
      });
    } catch (_) {
      _freeAll();
      rethrow;
    }

    _loaded = true;
  }

  /// Libère les buffers natifs et remet l'instance en état non-chargé.
  ///
  /// Sans effet si [load] n'a pas été appelé ou si déjà libéré.
  void dispose() {
    _freeAll();
    _loaded = false;
  }

  void _freeAll() {
    if (_kaldPtr != nullptr) {
      _kaldFinalizer.detach(this);
      calloc.free(_kaldPtr);
      _kaldPtr = nullptr;
      _kaldLen = 0;
    }
    if (_litsPtr != nullptr) {
      _litsFinalizer.detach(this);
      calloc.free(_litsPtr);
      _litsPtr = nullptr;
      _litsLen = 0;
    }
  }

  void _assertLoaded() {
    if (!_loaded) {
      throw StateError(
        'LiturgicalCalendar : appeler load() avant toute lecture.',
      );
    }
  }

  // ── Lecture ─────────────────────────────────────────────────────────────────

  /// Retourne les données liturgiques pour `(year, month, day)`.
  ///
  /// Effectue deux appels FFI en séquence : `kal_read_entry` puis
  /// `kal_read_feast` pour résoudre la fête principale.
  ///
  /// Retourne `null` si l'année est hors de la plage couverte par le `.kald`.
  /// [DayData.primary] est `null` pour un Padding Entry (pas de célébration).
  /// [DayData.secondaryIndices] est vide si aucune secondaire.
  DayData? getDay(int year, int month, int day) {
    _assertLoaded();
    final doy = toDoy(year, month, day);

    return using((arena) {
      // Slot Timeline.
      final tlPtr = arena<KalTimelineEntry>();
      final rcTl = _b.kalReadEntry(_kaldPtr, _kaldLen, year, doy, tlPtr);
      if (rcTl != kalEngineOk) return null;

      final tl = TimelineEntryData._fromFfi(tlPtr.ref);

      // Padding Entry — aucune fête à résoudre.
      if (tl.isPadding) {
        return DayData(timeline: tl, primary: null, secondaryIndices: const []);
      }

      // Fête principale.
      final feastPtr = arena<KalFeastEntry>();
      final rcFeast =
          _b.kalReadFeast(_kaldPtr, _kaldLen, tl.primaryIndex, feastPtr);
      if (rcFeast != kalEngineOk) return null;

      final primary = FeastEntryData._fromFfi(feastPtr.ref);

      // Célébrations secondaires (registry_indices bruts).
      final secondaries = <int>[];
      if (tl.secondaryCount > 0) {
        final secPtr = arena<Uint16>(tl.secondaryCount);
        final rcSec = _b.kalReadSecondary(
          _kaldPtr, _kaldLen,
          tl.secondaryOffset, tl.secondaryCount,
          secPtr, tl.secondaryCount,
        );
        if (rcSec == kalEngineOk) {
          for (int i = 0; i < tl.secondaryCount; i++) {
            secondaries.add(secPtr[i]);
          }
        }
      }

      return DayData(
        timeline:         tl,
        primary:          primary,
        secondaryIndices: List.unmodifiable(secondaries),
      );
    });
  }

  /// Résout un `registry_index` (1-based) en [FeastEntryData].
  ///
  /// Retourne `null` si l'index est nul (Padding sentinel) ou hors plage.
  FeastEntryData? getFeast(int registryIndex) {
    _assertLoaded();
    return using((arena) {
      final ptr = arena<KalFeastEntry>();
      final rc = _b.kalReadFeast(_kaldPtr, _kaldLen, registryIndex, ptr);
      if (rc != kalEngineOk) return null;
      return FeastEntryData._fromFfi(ptr.ref);
    });
  }

  /// Retourne le label et l'annotation d'une fête pour l'année donnée.
  ///
  /// Les `String` Dart sont construites par copie depuis le buffer `.lits`
  /// natif — le buffer peut être libéré après cet appel sans invalider les
  /// chaînes retournées.
  ///
  /// Retourne `null` si `(feastId, year)` est absent du corpus `.lits`.
  LitsEntryData? getLabel(int feastId, int year) {
    _assertLoaded();
    return using((arena) {
      final outLabelPtr = arena<Pointer<Uint8>>();
      final outLabelLen = arena<UintPtr>();
      final outAnnotPtr = arena<Pointer<Uint8>>();
      final outAnnotLen = arena<UintPtr>();

      final rc = _b.kalLitsGetLabel(
        _litsPtr, _litsLen, feastId, year,
        outLabelPtr, outLabelLen,
        outAnnotPtr, outAnnotLen,
      );

      if (rc != kalEngineOk) return null;

      // Copie en String Dart — les ptr pointent dans _litsPtr (zero-copy Rust,
      // copie Dart ici via toDartString).
      final label = outLabelPtr.value
          .cast<Utf8>()
          .toDartString(length: outLabelLen.value);

      String? annotation;
      if (outAnnotPtr.value != nullptr) {
        annotation = outAnnotPtr.value
            .cast<Utf8>()
            .toDartString(length: outAnnotLen.value);
      }

      return LitsEntryData(label: label, annotation: annotation);
    });
  }

  /// Scanne la plage `[yearFrom, yearTo]` et retourne les indices des slots
  /// dont `FeastEntry.flags & flagMask == flagValue`.
  ///
  /// Les indices sont encodés `(year − epoch) × 366 + doy`.
  ///
  /// Deux passes FFI : la première mesure le count, la seconde remplit le
  /// buffer. Retourne `const []` si aucun résultat.
  List<int> scanFlags({
    required int yearFrom,
    required int yearTo,
    required int flagMask,
    required int flagValue,
  }) {
    _assertLoaded();
    return using((arena) {
      final countPtr = arena<Uint32>();

      // Passe 1 : mesurer.
      _b.kalScanFlags(
        _kaldPtr, _kaldLen,
        yearFrom, yearTo, flagMask, flagValue,
        nullptr, 0, countPtr,
      );

      final count = countPtr.value;
      if (count == 0) return const <int>[];

      // Passe 2 : collecter.
      final indicesPtr = arena<Uint32>(count);
      _b.kalScanFlags(
        _kaldPtr, _kaldLen,
        yearFrom, yearTo, flagMask, flagValue,
        indicesPtr, count, countPtr,
      );

      return List<int>.generate(
        countPtr.value, // utiliser le count confirmé par la 2e passe
        (i) => indicesPtr[i],
        growable: false,
      );
    });
  }

  // ── Utilitaire public ────────────────────────────────────────────────────────

  /// Convertit `(year, month, day)` en day-of-year 0-based.
  ///
  /// Table fixe — **indépendante de la bissextilité**. Le slot 59 correspond
  /// au Padding Feb29 réservé dans le format `.kald` (présent toutes les
  /// années, même non-bissextiles).
  ///
  /// `month` ∈ [1, 12], `day` ∈ [1, 31].
  static int toDoy(int year, int month, int day) {
    assert(month >= 1 && month <= 12);
    assert(day >= 1 && day <= 31);
    return _kMonthOffsets[month - 1] + (day - 1);
  }

  static const _kMonthOffsets = [
    0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335,
  ];
}
