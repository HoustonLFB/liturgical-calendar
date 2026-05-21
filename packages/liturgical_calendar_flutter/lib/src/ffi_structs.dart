import 'dart:ffi';

/// `TimelineEntry` C-ABI — 8 octets.
///
/// Représente l'occurrence journalière pour un slot `(year, doy)`.
/// `primaryIndex == 0` est la sentinelle Padding Entry (aucune fête propre).
///
/// v6 — offsets confirmés par `LAYOUT_DISCRIMINANT` :
/// - +0 `primaryIndex`    Uint16
/// - +2 `secondaryOffset` Uint16
/// - +4 `occurrenceFlags` Uint8   (bit0=vesperaeI, bit1=vigilia, bits[4:2]=LiturgicalPeriod)
/// - +5 `secondaryCount`  Uint8
/// - +6 `liturgicalWeek`  Uint8   (0=N/A, 1–34=ordinal semaine liturgique)
/// - +7 `reserved`        Uint8   (nul)
///
/// Changement v5→v6 : `reserved: Uint16` (bytes 6–7) remplacé par
/// `liturgicalWeek: Uint8` (byte 6) + `reserved: Uint8` (byte 7).
/// Les Padding Entries portent `occurrenceFlags[4:2]` et `liturgicalWeek`
/// même sans fête propre (sauf DOY 59 non-bissextile : zéro absolu).
final class KalTimelineEntry extends Struct {
  @Uint16()
  external int primaryIndex;

  @Uint16()
  external int secondaryOffset;

  @Uint8()
  external int occurrenceFlags;

  @Uint8()
  external int secondaryCount;

  @Uint8()
  external int liturgicalWeek;

  @Uint8()
  external int reserved;
}

/// `FeastEntry` C-ABI — 4 octets.
///
/// Invariants d'une fête dans le Feast Registry (indexé 1-based).
///
/// `flags` layout v6 :
/// - bits [3:0]   → Precedence      (0–12)
/// - bits [7:4]   → LiturgicalColor (0–5)
/// - bits [10:8]  → réservés, nuls  (v5 : LiturgicalPeriod — supprimé en v6)
/// - bits [13:11] → Nature          (0–4)
/// - bit  [14]    → hasVigilMass
/// - bit  [15]    → réservé
///
/// La `LiturgicalPeriod` est désormais portée par
/// `KalTimelineEntry.occurrenceFlags[4:2]`.
final class KalFeastEntry extends Struct {
  @Uint16()
  external int feastId;

  @Uint16()
  external int flags;
}

/// Header `.kald` v6 C-ABI — 80 octets.
///
/// Passé en sortie à `kal_validate_header`. Seuls `epoch`, `range`,
/// `entryCount` et `checksum[0..8]` sont lus côté Dart ; les autres
/// champs sont réservés à l'Engine Rust.
final class KalHeader extends Struct {
  /// `b"KALD"`
  @Array(4)
  external Array<Uint8> magic;

  /// Version du format (6 pour v6).
  @Uint16()
  external int version;

  @Uint16()
  external int variantId;

  /// Première année couverte par ce fichier.
  @Uint16()
  external int epoch;

  /// Nombre d'années couvertes.
  @Uint16()
  external int range;

  /// Nombre total de `TimelineEntry` (range × 366).
  @Uint32()
  external int entryCount;

  /// Offset en octets du Secondary Pool depuis le début du fichier.
  @Uint32()
  external int poolOffset;

  /// Taille en octets du Secondary Pool.
  @Uint32()
  external int poolSize;

  /// Offset en octets du Feast Registry depuis le début du fichier.
  @Uint32()
  external int registryOffset;

  /// Nombre de `FeastEntry` dans le Feast Registry.
  @Uint32()
  external int registryCount;

  @Uint16()
  external int feastIdBase;

  @Uint16()
  external int headerReserved;

  /// SHA-256 du payload (Registry + Timeline + Pool).
  /// `checksum[0..8]` doit correspondre au `build_id` du `.lits` associé.
  @Array(32)
  external Array<Uint8> checksum;

  /// Discriminant de layout v6 — empreinte des offsets de `TimelineEntry`.
  @Array(8)
  external Array<Uint8> layoutDiscriminant;

  @Array(4)
  external Array<Uint8> reserved2;
}
