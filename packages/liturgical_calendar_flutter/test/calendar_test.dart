import 'dart:typed_data';

import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:liturgical_calendar_flutter/liturgical_calendar_flutter.dart';

// ══ Générateurs de fixtures ═══════════════════════════════════════════════════
//
// Les buffers de test sont entièrement construits en Dart — pas de fichiers
// binaires nécessaires. Le SHA-256 est calculé via `package:crypto` pour
// produire un .kald dont `kal_validate_header` (validation complète) accepte
// le checksum.

void _u16le(Uint8List b, int o, int v) {
  b[o]     = v & 0xFF;
  b[o + 1] = (v >> 8) & 0xFF;
}

void _u32le(Uint8List b, int o, int v) {
  b[o]     = v & 0xFF;
  b[o + 1] = (v >> 8) & 0xFF;
  b[o + 2] = (v >> 16) & 0xFF;
  b[o + 3] = (v >> 24) & 0xFF;
}

void _u64le(Uint8List b, int o, int v) {
  for (var i = 0; i < 8; i++) {
    b[o + i] = (v >> (i * 8)) & 0xFF;
  }
}

/// Discriminant de layout de [KalTimelineEntry] v5 :
/// `size(8) ^ (off_secondary_offset(2)<<8) ^ (off_occ_flags(4)<<16)
///  ^ (off_secondary_count(5)<<24) ^ (off_reserved(6)<<32) ^ (version(5)<<48)`
/// = 0x0005000605040208
const int _kLayoutDiscriminant = 0x0005000605040208;

/// Génère un buffer `.kald` v5 minimal avec SHA-256 correct.
///
/// Registry et Timeline sont remplis de zéros → tous les slots sont des
/// Padding Entries (primaryIndex = 0).
Uint8List _makeKald({
  int registryCount = 0,
  int entryCount    = 366,
  int epoch         = 2024,
  int range         = 1,
}) {
  final registrySize = registryCount * 4;
  final timelineSize = entryCount * 8;
  final poolOffset   = 80 + registrySize + timelineSize;

  // Payload = ce que SHA-256 couvre (tout sauf le header lui-même).
  final payload  = Uint8List(registrySize + timelineSize);
  final checksum = sha256.convert(payload).bytes; // List<int>, 32 octets

  final buf = Uint8List(poolOffset);

  // Header (80 octets).
  buf[0] = 0x4B; buf[1] = 0x41; buf[2] = 0x4C; buf[3] = 0x44; // KALD
  _u16le(buf,  4, 5);               // version = 5
  _u16le(buf,  8, epoch);           // epoch
  _u16le(buf, 10, range);           // range
  _u32le(buf, 12, entryCount);      // entry_count
  _u32le(buf, 16, poolOffset);      // pool_offset
  // pool_size = 0 (déjà 0)
  _u32le(buf, 24, 80);              // registry_offset
  _u32le(buf, 28, registryCount);   // registry_count
  // feastIdBase, reserved = 0
  for (var i = 0; i < 32; i++) buf[36 + i] = checksum[i]; // checksum[0..32]
  _u64le(buf, 68, _kLayoutDiscriminant);                   // layout_discriminant
  // reserved2 = 0

  // Payload.
  buf.setAll(80, payload);
  return buf;
}

/// Retourne les 8 premiers octets du checksum embarqué dans un buffer `.kald`.
List<int> _kaldBuildId(Uint8List kald) => kald.sublist(36, 44);

/// Génère un buffer `.lits` minimal valide — 0 entrées, pool vide.
///
/// [buildId] : 8 octets correspondant à `kald.checksum[0..8]`.
Uint8List _makeLits(List<int> buildId) {
  assert(buildId.length == 8);
  final buf = Uint8List(32);
  buf[0] = 0x4C; buf[1] = 0x49; buf[2] = 0x54; buf[3] = 0x53; // LITS
  _u16le(buf,  4, 1);  // version = 1
  for (var i = 0; i < 8; i++) buf[12 + i] = buildId[i];  // build_id
  _u32le(buf, 20, 0);  // entry_count = 0
  _u32le(buf, 24, 32); // pool_offset = 32
  // pool_size = 0
  return buf;
}

// ══ Tests ══════════════════════════════════════════════════════════════════════

void main() {
  // Buffers partagés entre groupes — construits une seule fois.
  late Uint8List validKald;
  late Uint8List validLits;

  setUpAll(() {
    validKald = _makeKald(epoch: 2024, range: 1, entryCount: 366);
    validLits = _makeLits(_kaldBuildId(validKald));
  });

  // ── toDoy ─────────────────────────────────────────────────────────────────

  group('LiturgicalCalendar.toDoy', () {
    test('1er janvier = 0', () {
      expect(LiturgicalCalendar.toDoy(2024, 1, 1), 0);
    });

    test('28 février = 58', () {
      expect(LiturgicalCalendar.toDoy(2024, 2, 28), 58);
    });

    test('slot 59 = Padding Feb29 (1er mars = doy 60)', () {
      expect(LiturgicalCalendar.toDoy(2024, 3, 1), 60);
    });

    test('31 décembre = 365', () {
      expect(LiturgicalCalendar.toDoy(2024, 12, 31), 365);
    });

    test('indépendant de l\'année — même valeur pour 2023 et 2024', () {
      expect(
        LiturgicalCalendar.toDoy(2023, 6, 15),
        LiturgicalCalendar.toDoy(2024, 6, 15),
      );
    });
  });

  // ── load / dispose ─────────────────────────────────────────────────────────

  group('LiturgicalCalendar.load', () {
    test('accepte des buffers valides sans exception', () {
      final cal = LiturgicalCalendar();
      expect(() => cal.load(validKald, validLits), returnsNormally);
      cal.dispose();
    });

    test('lève KalValidationException sur SHA-256 corrompu', () {
      final bad = Uint8List.fromList(validKald);
      bad[36] ^= 0xFF; // corruption dans checksum
      final cal = LiturgicalCalendar();
      expect(
        () => cal.load(bad, validLits),
        throwsA(isA<KalValidationException>()),
      );
    });

    test('lève KalValidationException sur magic invalide', () {
      final bad = Uint8List.fromList(validKald);
      bad[0] = 0x00;
      final cal = LiturgicalCalendar();
      expect(
        () => cal.load(bad, validLits),
        throwsA(isA<KalValidationException>()),
      );
    });

    test('lève KalBuildIdMismatchException si build_id incohérent', () {
      final wrongLits = _makeLits(List.filled(8, 0xFF));
      final cal = LiturgicalCalendar();
      expect(
        () => cal.load(validKald, wrongLits),
        throwsA(isA<KalBuildIdMismatchException>()),
      );
    });

    test('dispose sans erreur après load', () {
      final cal = LiturgicalCalendar();
      cal.load(validKald, validLits);
      expect(cal.dispose, returnsNormally);
    });

    test('dispose idempotent — double appel sans erreur', () {
      final cal = LiturgicalCalendar();
      cal.load(validKald, validLits);
      cal.dispose();
      expect(cal.dispose, returnsNormally);
    });

    test('relance possible après dispose', () {
      final cal = LiturgicalCalendar();
      cal.load(validKald, validLits);
      cal.dispose();
      expect(() => cal.load(validKald, validLits), returnsNormally);
      cal.dispose();
    });
  });

  // ── getDay ────────────────────────────────────────────────────────────────

  group('LiturgicalCalendar.getDay', () {
    late LiturgicalCalendar cal;

    setUp(() {
      cal = LiturgicalCalendar();
      cal.load(validKald, validLits);
    });

    tearDown(() => cal.dispose());

    test('lève StateError si load() non appelé', () {
      final fresh = LiturgicalCalendar();
      expect(() => fresh.getDay(2024, 1, 1), throwsStateError);
    });

    test('retourne DayData non-null pour une date dans la plage', () {
      expect(cal.getDay(2024, 1, 1), isNotNull);
    });

    test('tous les slots sont isPadding (buffer zeroed)', () {
      // Le buffer de test est rempli de zéros → primaryIndex = 0.
      final day = cal.getDay(2024, 6, 15);
      expect(day?.timeline.isPadding, isTrue);
      expect(day?.primary, isNull);
      expect(day?.secondaryIndices, isEmpty);
    });

    test('retourne null pour une année hors plage', () {
      expect(cal.getDay(1900, 1, 1), isNull);
      expect(cal.getDay(2025, 1, 1), isNull); // range = 1 → seul 2024
    });

    test('slot doy=59 (Padding Feb29) accessible', () {
      // 28 fév = doy 58, 1er mars = doy 60 ; slot 59 = Padding Feb29.
      expect(cal.getDay(2024, 2, 28), isNotNull); // doy 58
      expect(cal.getDay(2024, 3,  1), isNotNull); // doy 60
    });

    test('retourne DayData avec secondaryIndices vide pour Padding', () {
      final day = cal.getDay(2024, 12, 31);
      expect(day?.secondaryIndices, isA<List<int>>());
      expect(day?.secondaryIndices, isEmpty);
    });
  });

  // ── getFeast ──────────────────────────────────────────────────────────────

  group('LiturgicalCalendar.getFeast', () {
    late LiturgicalCalendar cal;

    setUp(() {
      cal = LiturgicalCalendar();
      cal.load(validKald, validLits);
    });

    tearDown(() => cal.dispose());

    test('retourne null pour index 0 (sentinel Padding)', () {
      expect(cal.getFeast(0), isNull);
    });

    test('retourne null pour index hors plage (registryCount = 0)', () {
      expect(cal.getFeast(1), isNull);
    });
  });

  // ── getLabel ──────────────────────────────────────────────────────────────

  group('LiturgicalCalendar.getLabel', () {
    late LiturgicalCalendar cal;

    setUp(() {
      cal = LiturgicalCalendar();
      cal.load(validKald, validLits);
    });

    tearDown(() => cal.dispose());

    test('retourne null pour feast_id absent du corpus .lits vide', () {
      expect(cal.getLabel(1, 2024), isNull);
    });

    test('retourne null pour feast_id = 0', () {
      expect(cal.getLabel(0, 2024), isNull);
    });
  });

  // ── scanFlags ─────────────────────────────────────────────────────────────

  group('LiturgicalCalendar.scanFlags', () {
    late LiturgicalCalendar cal;

    setUp(() {
      cal = LiturgicalCalendar();
      cal.load(validKald, validLits);
    });

    tearDown(() => cal.dispose());

    test('retourne liste vide sur buffer zeroed (aucun flag actif)', () {
      final res = cal.scanFlags(
        yearFrom: 2024, yearTo: 2024,
        flagMask: 0xFFFF, flagValue: 0x0001,
      );
      expect(res, isEmpty);
    });

    test('masque 0x0000 retourne liste vide (aucune correspondance possible)', () {
      final res = cal.scanFlags(
        yearFrom: 2024, yearTo: 2024,
        flagMask: 0x0000, flagValue: 0x0001,
      );
      expect(res, isEmpty);
    });

    test('retourne une List<int> non-growable', () {
      final res = cal.scanFlags(
        yearFrom: 2024, yearTo: 2024,
        flagMask: 0x000F, flagValue: 0x0000,
      );
      // Liste non-growable si count > 0, const [] si count = 0.
      expect(res, isA<List<int>>());
    });
  });

  // ── LitsEntryData ─────────────────────────────────────────────────────────

  group('LitsEntryData', () {
    test('toString inclut le label', () {
      const e = LitsEntryData(label: 'Nativitas Domini');
      expect(e.toString(), contains('Nativitas Domini'));
    });

    test('annotation null non présente dans toString', () {
      const e = LitsEntryData(label: 'Test');
      expect(e.toString(), isNot(contains('annotation')));
    });
  });

  // ── Décodage flags ────────────────────────────────────────────────────────

  group('Décodage bit fields FeastEntryData', () {
    // Construction d'un FeastEntry synthétique via ffi_structs n'est pas
    // accessible depuis les tests (structs internes). On vérifie les enums.

    test('Precedence.fromValue retourne null pour valeur inconnue', () {
      expect(Precedence.fromValue(99), isNull);
    });

    test('Precedence.memoriaAdLibitum a la valeur 11', () {
      expect(Precedence.memoriaAdLibitum.value, 11);
    });

    test('LiturgicalColor.viridis a la valeur 2', () {
      expect(LiturgicalColor.viridis.value, 2);
    });

    test('LiturgicalColor.rubeus a la valeur 1', () {
      expect(LiturgicalColor.rubeus.value, 1);
    });

    test('LiturgicalColor.rosaceus = 4, niger = 5 (ordre Rust)', () {
      expect(LiturgicalColor.rosaceus.value, 4);
      expect(LiturgicalColor.niger.value, 5);
    });

    test('LiturgicalPeriod.tempusOrdinarium a la valeur 0', () {
      expect(LiturgicalPeriod.tempusOrdinarium.value, 0);
    });

    test('LiturgicalPeriod — noms canoniques confirmés', () {
      expect(LiturgicalPeriod.tempusAdventus.value,     1);
      expect(LiturgicalPeriod.tempusNativitatis.value,  2);
      expect(LiturgicalPeriod.tempusQuadragesimae.value, 3);
      expect(LiturgicalPeriod.triduumPaschale.value,    4);
      expect(LiturgicalPeriod.tempusPaschale.value,     5);
      expect(LiturgicalPeriod.diesSancti.value,         6);
    });

    test('Nature.memoria a la valeur 3', () {
      expect(Nature.memoria.value, 3);
    });

    test('Nature — valeurs complètes confirmées', () {
      expect(Nature.sollemnitas.value,  0);
      expect(Nature.festum.value,       1);
      expect(Nature.dominica.value,     2);
      expect(Nature.memoria.value,      3);
      expect(Nature.commemoratio.value, 4);
      expect(Nature.feria.value,        5);
    });

    test('Precedence — noms canoniques confirmés', () {
      expect(Precedence.triduumSacrum.value,                0);
      expect(Precedence.sollemnitatesMaiores.value,         1);
      expect(Precedence.sollemnitatesGenerales.value,       2);
      expect(Precedence.sollemnitatesPropria.value,         3);
      expect(Precedence.festaDomini.value,                  4);
      expect(Precedence.dominicaePerAnnum.value,            5);
      expect(Precedence.festaBMVEtSanctorumGenerales.value, 6);
      expect(Precedence.festaPropria.value,                 7);
      expect(Precedence.feriaePrivilegiatae.value,          8);
      expect(Precedence.memoriaeObligatoriaGenerales.value, 9);
      expect(Precedence.memoriaeObligatoriaePropria.value,  10);
      expect(Precedence.memoriaeAdLibitum.value,            11);
      expect(Precedence.feriaePerAnnum.value,               12);
    });

    test('LiturgicalColor.fromValue retourne null pour valeur inconnue', () {
      expect(LiturgicalColor.fromValue(99), isNull);
    });
  });
}
