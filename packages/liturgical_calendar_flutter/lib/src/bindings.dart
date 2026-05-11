import 'dart:ffi';
import 'dart:io';

import 'ffi_structs.dart';

// ── Signatures natives (NativeType) ──────────────────────────────────────────

typedef _ValidateHeaderNative = Int32 Function(
  Pointer<Uint8> buf,
  UintPtr len,
  Pointer<KalHeader> outHeader,
);
typedef _ValidateHeaderDart = int Function(
  Pointer<Uint8> buf,
  int len,
  Pointer<KalHeader> outHeader,
);

typedef _ReadEntryNative = Int32 Function(
  Pointer<Uint8> buf,
  UintPtr len,
  Uint16 year,
  Uint16 doy,
  Pointer<KalTimelineEntry> out,
);
typedef _ReadEntryDart = int Function(
  Pointer<Uint8> buf,
  int len,
  int year,
  int doy,
  Pointer<KalTimelineEntry> out,
);

typedef _ReadFeastNative = Int32 Function(
  Pointer<Uint8> buf,
  UintPtr len,
  Uint16 registryIndex,
  Pointer<KalFeastEntry> out,
);
typedef _ReadFeastDart = int Function(
  Pointer<Uint8> buf,
  int len,
  int registryIndex,
  Pointer<KalFeastEntry> out,
);

typedef _ReadSecondaryNative = Int32 Function(
  Pointer<Uint8> buf,
  UintPtr len,
  Uint16 secondaryOffset,
  Uint8 count,
  Pointer<Uint16> outIndices,
  Uint8 capacity,
);
typedef _ReadSecondaryDart = int Function(
  Pointer<Uint8> buf,
  int len,
  int secondaryOffset,
  int count,
  Pointer<Uint16> outIndices,
  int capacity,
);

typedef _ScanFlagsNative = Int32 Function(
  Pointer<Uint8> buf,
  UintPtr len,
  Uint16 yearFrom,
  Uint16 yearTo,
  Uint16 flagMask,
  Uint16 flagValue,
  Pointer<Uint32> outIndices,
  Uint32 outCapacity,
  Pointer<Uint32> outCount,
);
typedef _ScanFlagsDart = int Function(
  Pointer<Uint8> buf,
  int len,
  int yearFrom,
  int yearTo,
  int flagMask,
  int flagValue,
  Pointer<Uint32> outIndices,
  int outCapacity,
  Pointer<Uint32> outCount,
);

typedef _LitsGetLabelNative = Int32 Function(
  Pointer<Uint8> litsBytes,
  UintPtr litsLen,
  Uint16 feastId,
  Uint16 year,
  Pointer<Pointer<Uint8>> outLabelPtr,
  Pointer<UintPtr> outLabelLen,
  Pointer<Pointer<Uint8>> outAnnotationPtr,
  Pointer<UintPtr> outAnnotationLen,
);
typedef _LitsGetLabelDart = int Function(
  Pointer<Uint8> litsBytes,
  int litsLen,
  int feastId,
  int year,
  Pointer<Pointer<Uint8>> outLabelPtr,
  Pointer<UintPtr> outLabelLen,
  Pointer<Pointer<Uint8>> outAnnotationPtr,
  Pointer<UintPtr> outAnnotationLen,
);

// ── Chargement ────────────────────────────────────────────────────────────────

DynamicLibrary _openLib() {
  if (Platform.isAndroid || Platform.isLinux) {
    return DynamicLibrary.open('libkal_flutter.so');
  }
  if (Platform.isWindows) {
    return DynamicLibrary.open('kal_flutter.dll');
  }
  if (Platform.isMacOS) {
    return DynamicLibrary.open('libkal_flutter.dylib');
  }
  throw UnsupportedError(
    'liturgical_calendar : plateforme non supportée '
    '(${Platform.operatingSystem})',
  );
}

// ── KalBindings ───────────────────────────────────────────────────────────────

/// Singleton qui résout et expose les symboles C-ABI de la bibliothèque native.
///
/// La résolution est faite une seule fois à la construction — toute erreur
/// de symbole manquant est détectée immédiatement.
///
/// Pour les tests, créer une instance avec [KalBindings.fromLib] en passant
/// une [DynamicLibrary] préchargée depuis le système de fichiers de test.
class KalBindings {
  KalBindings._init(DynamicLibrary lib)
      : kalValidateHeader = lib.lookupFunction<
            _ValidateHeaderNative, _ValidateHeaderDart>(
          'kal_validate_header',
        ),
        kalReadEntry = lib.lookupFunction<_ReadEntryNative, _ReadEntryDart>(
          'kal_read_entry',
        ),
        kalReadFeast = lib.lookupFunction<_ReadFeastNative, _ReadFeastDart>(
          'kal_read_feast',
        ),
        kalReadSecondary =
            lib.lookupFunction<_ReadSecondaryNative, _ReadSecondaryDart>(
          'kal_read_secondary',
        ),
        kalScanFlags =
            lib.lookupFunction<_ScanFlagsNative, _ScanFlagsDart>(
          'kal_scan_flags',
        ),
        kalLitsGetLabel =
            lib.lookupFunction<_LitsGetLabelNative, _LitsGetLabelDart>(
          'kal_lits_get_label',
        );

  /// Instance singleton — bibliothèque résolue depuis le chemin système.
  static final KalBindings instance = KalBindings._init(_openLib());

  /// Constructeur pour les tests : inject une [DynamicLibrary] arbitraire.
  factory KalBindings.fromLib(DynamicLibrary lib) => KalBindings._init(lib);

  final _ValidateHeaderDart  kalValidateHeader;
  final _ReadEntryDart       kalReadEntry;
  final _ReadFeastDart       kalReadFeast;
  final _ReadSecondaryDart   kalReadSecondary;
  final _ScanFlagsDart       kalScanFlags;
  final _LitsGetLabelDart    kalLitsGetLabel;
}
