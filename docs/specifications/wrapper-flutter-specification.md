# Spécification — `liturgical-calendar-flutter` v2 (format `.kald` v5)

> Remplace la spec v1 (incompatible avec le breaking change v4→v5).  
> Base : `liturgical-calendar-core` v5 — `TimelineEntry` + `FeastEntry` + `LitsProvider`.

---

## 1. Périmètre

### 1.1 Crate Rust — `./crates/liturgical-calendar-flutter/`

`cdylib` minimal, `no_std`. **N'ajoute qu'une seule fonction** C-ABI que le Core ne fournit pas : `kal_lits_get_label`.

Les six fonctions Core (`kal_validate_header`, `kal_validate_header_fast`, `kal_read_entry`, `kal_read_feast`, `kal_read_secondary`, `kal_scan_flags`) sont exportées automatiquement dans le `.so`/`.dll`/`.dylib` compilé, car elles portent `#[unsafe(no_mangle)]` dans un `rlib` lié statiquement. Aucun shim redondant.

### 1.2 Package Flutter — `./crates/liturgical_calendar/`

Couche Dart pure (`dart:ffi`). Pas de `flutter_rust_bridge`. Publiable sur pub.dev.

---

## 2. Unique fonction Rust ajoutée

### `kal_lits_get_label`

```rust
/// Résout (feast_id, year) → (label, annotation) depuis un buffer .lits.
///
/// Pointeurs de sortie : adresses absolues dans lits_bytes (zero-copy).
/// out_annotation_ptr = null / out_annotation_len = 0 si annotation absente.
///
/// Codes d'erreur :
///   KAL_ERR_NULL_PTR      (-1) : out_label_ptr ou out_label_len est null
///   KAL_ERR_BUF_TOO_SMALL (-2) : buffer < 32 octets
///   KAL_ERR_MAGIC         (-3) : magic != b"LITS"
///   KAL_ERR_VERSION       (-4) : version != 1
///   KAL_ERR_FILE_SIZE     (-6) : pool_offset / pool_size incohérents
///   KAL_ERR_INDEX_OOB     (-7) : (feast_id, year) absent du corpus
///
/// # Safety
/// lits_bytes valide pour lits_len octets.
/// out_label_ptr et out_label_len non-NULL.
/// out_annotation_ptr et out_annotation_len peuvent être NULL (champs ignorés).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kal_lits_get_label(
    lits_bytes:         *const u8,
    lits_len:           usize,
    feast_id:           u16,
    year:               u16,
    out_label_ptr:      *mut *const u8,  // non-NULL
    out_label_len:      *mut usize,      // non-NULL
    out_annotation_ptr: *mut *const u8,  // nullable
    out_annotation_len: *mut usize,      // nullable
) -> i32
```

**Mapping `LitsError` → code ABI :**

| `LitsError`           | Code ABI |
|-----------------------|----------|
| `BufferTooShort`      | `-2`     |
| `InvalidMagic`        | `-3`     |
| `UnsupportedVersion`  | `-4`     |
| `CorruptLayout`       | `-6`     |
| `.get()` retourne `None` | `-7` |

**Comportement annotation absente :**  
Si `LitsEntry.annotation == None` : écrire `null` dans `*out_annotation_ptr` (si non-NULL) et `0` dans `*out_annotation_len` (si non-NULL). Dart teste `out_annotation_ptr == nullptr` pour discriminer.

---

## 3. Symboles Dart disponibles dans la bibliothèque compilée

| Symbole C               | Source  | Rôle                                          |
|-------------------------|---------|-----------------------------------------------|
| `kal_validate_header`   | Core    | Validation SHA-256 — appel unique à l'ouverture |
| `kal_validate_header_fast` | Core | Validation structurelle — O(1), sans SHA-256  |
| `kal_read_entry`        | Core    | `TimelineEntry` pour `(year, doy)`            |
| `kal_read_feast`        | Core    | `FeastEntry` pour un `registry_index`         |
| `kal_read_secondary`    | Core    | Array de `registry_index` depuis le Secondary Pool |
| `kal_scan_flags`        | Core    | Scan `FeastEntry.flags` sur plage d'années    |
| `kal_lits_get_label`    | Flutter | Résolution `(feast_id, year)` → label/annotation |

---

## 4. Types Dart FFI

### 4.1 Structs `dart:ffi` (layout C-ABI)

**`KalTimelineEntry` — 8 octets**
```dart
final class KalTimelineEntry extends Struct {
  @Uint16() external int primaryIndex;      // 0 = Padding Entry
  @Uint16() external int secondaryOffset;   // offset en u16 dans le Secondary Pool
  @Uint8()  external int occurrenceFlags;   // bit0 = vesperaeI, bit1 = vigilia
  @Uint8()  external int secondaryCount;
  @Uint16() external int reserved;
}
```

**`KalFeastEntry` — 4 octets**
```dart
final class KalFeastEntry extends Struct {
  @Uint16() external int feastId;
  @Uint16() external int flags;
  // bits [3:0]   = Precedence
  // bits [7:4]   = Color
  // bits [10:8]  = LiturgicalPeriod
  // bits [13:11] = Nature
  // bit  [14]    = hasVigilMass
}
```

**`KalHeader` — 80 octets**
```dart
final class KalHeader extends Struct {
  @Array(4)  external Array<Uint8> magic;
  @Uint16()  external int version;
  @Uint16()  external int variantId;
  @Uint16()  external int epoch;
  @Uint16()  external int range;
  @Uint32()  external int entryCount;
  @Uint32()  external int poolOffset;
  @Uint32()  external int poolSize;
  @Uint32()  external int registryOffset;
  @Uint32()  external int registryCount;
  @Uint16()  external int feastIdBase;
  @Uint16()  external int reserved;
  @Array(32) external Array<Uint8> checksum;
  @Array(8)  external Array<Uint8> layoutDiscriminant;
  @Array(4)  external Array<Uint8> reserved2;
}
```

### 4.2 Classes Dart (données copiées depuis FFI, immutables)

**`TimelineEntryData`**
```dart
class TimelineEntryData {
  final int  primaryIndex;
  final int  secondaryOffset;
  final int  secondaryCount;
  final bool hasVesperaeI;   // occurrenceFlags & 0x01
  final bool hasVigilia;     // occurrenceFlags & 0x02
  bool get isPadding => primaryIndex == 0;
}
```

**`FeastEntryData`**
```dart
class FeastEntryData {
  final int               feastId;
  final Precedence        precedence;
  final LiturgicalColor   color;
  final LiturgicalPeriod  period;
  final Nature            nature;
  final bool              hasVigilMass;
}
```

**`DayData`** — résultat de `getDay`
```dart
class DayData {
  final TimelineEntryData timeline;
  final FeastEntryData?   primary;           // null si isPadding
  final List<int>         secondaryIndices;  // registry_indices bruts
}
```

**`LitsEntryData`**
```dart
class LitsEntryData {
  final String  label;
  final String? annotation;
}
```

### 4.3 Énumérations

Extraites par masques sur `KalFeastEntry.flags` :

```dart
enum Precedence {
  // valeurs 0–12, noms canoniques du rite
}
enum LiturgicalColor {
  // valeurs 0–5 — NE PAS nommer Color (collision avec Flutter SDK)
}
enum LiturgicalPeriod {
  // valeurs 0–6
}
enum Nature {
  // valeurs 0–4
}
```

Décoder via fonction statique, retourner `null` sur valeur inconnue (forward-compat).

---

## 5. Classe `LiturgicalCalendar`

### 5.1 État interne

```dart
class LiturgicalCalendar {
  Pointer<Uint8> _kaldPtr = nullptr;
  int            _kaldLen = 0;
  Pointer<Uint8> _litsPtr = nullptr;
  int            _litsLen = 0;
  bool           _loaded  = false;

  static final _finalizer = NativeFinalizer(calloc.nativeFree);
}
```

### 5.2 `Future<void> load(Uint8List kaldData, Uint8List litsData)`

Séquence :

1. Allouer `_kaldPtr = calloc<Uint8>(kaldData.length)`, copier, stocker `_kaldLen`.  
2. Allouer `_litsPtr = calloc<Uint8>(litsData.length)`, copier, stocker `_litsLen`.  
3. Attacher `_finalizer` aux deux pointeurs.  
4. Allouer `KalHeader` sur tas temporaire, appeler `kal_validate_header(_kaldPtr, _kaldLen, headerPtr)` — lever `KalValidationException(code)` si échec.  
5. **Vérification `build_id`** : lire `_litsPtr[12..20]` et comparer avec `headerPtr.checksum[0..8]` octet par octet. Lever `KalBuildIdMismatchException` si incohérence.  
6. Marquer `_loaded = true`.

> Note : la vérification `build_id` est implémentée en Dart pur (lecture directe des `Pointer<Uint8>`), sans appel Rust supplémentaire.

### 5.3 `DayData? getDay(int year, int month, int day)`

1. `doy = _toDoy(year, month, day)`.  
2. Allouer `KalTimelineEntry` sur le tas, appeler `kal_read_entry` — retourner `null` si erreur.  
3. Copier en `TimelineEntryData`.  
4. Si `isPadding` → retourner `DayData(timeline, null, const [])`.  
5. Allouer `KalFeastEntry`, appeler `kal_read_feast(primaryIndex)` — retourner `null` si erreur.  
6. Si `secondaryCount > 0` : allouer buffer `secondaryCount × Uint16`, appeler `kal_read_secondary(secondaryOffset, secondaryCount)`, copier en `List<int>`.  
7. Retourner `DayData` composé, libérer les allocations temporaires.

### 5.4 `FeastEntryData? getFeast(int registryIndex)`

Appel direct à `kal_read_feast`. Retourner `null` si erreur (registryIndex hors plage, Padding, etc.).

### 5.5 `LitsEntryData? getLabel(int feastId, int year)`

1. Allouer 4 pointeurs de sortie (`Pointer<Pointer<Uint8>>` × 2, `Pointer<Size>` × 2).  
2. Appeler `kal_lits_get_label`.  
3. Si `KAL_ENGINE_OK` :  
   - Construire `label` via `outLabelPtr.value.cast<Utf8>().toDartString(length: outLabelLen.value)`.  
   - Si `outAnnotationPtr.value != nullptr` : construire `annotation` idem.  
4. Retourner `null` si code < 0.  
5. Libérer les 4 pointeurs de sortie.

### 5.6 `List<int> scanFlags(int yearFrom, int yearTo, int flagMask, int flagValue)`

Two-pass :
1. Appeler `kal_scan_flags(..., nullptr, 0, countPtr)` → obtenir `totalCount`.  
2. Si `totalCount == 0` → retourner `const []`.  
3. Allouer `totalCount × Uint32`, rappeler `kal_scan_flags(..., indicesPtr, totalCount, countPtr)`.  
4. Copier en `List<int>`, libérer.

### 5.7 `void dispose()`

Libérer `_kaldPtr` et `_litsPtr` via `calloc.free`. Positionner `_loaded = false`.

### 5.8 `_toDoy(int year, int month, int day) → int`

```dart
static const _monthOffsets = [0,31,60,91,121,152,182,213,244,274,305,335];
static int _toDoy(int year, int month, int day) =>
    _monthOffsets[month - 1] + (day - 1);
```

Table fixe — indépendante de la bissextilité, conforme à la spec `.kald` v5 (slot 59 = Padding Feb29).

---

## 6. Chargement de la bibliothèque native

```dart
// bindings.dart
DynamicLibrary _openLib() {
  if (Platform.isAndroid || Platform.isLinux)
    return DynamicLibrary.open('libkal_flutter.so');
  if (Platform.isWindows)
    return DynamicLibrary.open('kal_flutter.dll');
  if (Platform.isMacOS)
    return DynamicLibrary.open('libkal_flutter.dylib');
  if (Platform.isIOS)
    return DynamicLibrary.process(); // lié statiquement sur iOS
  throw UnsupportedError('Plateforme non supportée');
}
```

---

## 7. Exceptions

```dart
class KalException implements Exception {
  final int code;
  const KalException(this.code);
}

class KalValidationException extends KalException {
  const KalValidationException(super.code);
}

class KalBuildIdMismatchException extends KalException {
  const KalBuildIdMismatchException() : super(-22);
}
```

---

## 8. Structure de fichiers

```
./crates/
│
├── liturgical-calendar-flutter/
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs                  (kal_lits_get_label uniquement)
│
└── liturgical_calendar/
    ├── pubspec.yaml
    ├── README.md
    ├── lib/
    │   ├── liturgical_calendar.dart  (barrel export)
    │   └── src/
    │       ├── bindings.dart         (DynamicLibrary + typedef FFI)
    │       ├── ffi_structs.dart      (KalTimelineEntry, KalFeastEntry, KalHeader)
    │       ├── calendar.dart         (LiturgicalCalendar)
    │       ├── types.dart            (enums + data classes)
    │       └── error_codes.dart      (constantes KAL_ERR_*)
    ├── test/
    │   ├── calendar_test.dart
    │   └── fixtures/
    │       ├── minimal.kald          (buffer de test minimal valide)
    │       └── minimal.lits          (buffer de test minimal valide)
    ├── android/
    ├── ios/
    ├── linux/
    ├── windows/
    └── macos/
```

---

## 9. `Cargo.toml` du crate Flutter

```toml
[package]
name    = "liturgical-calendar-flutter"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
liturgical-calendar-core = { path = "../../" }

[profile.release]
opt-level      = "z"
lto            = true
codegen-units  = 1
strip          = true
```

---

## 10. Points ouverts — décisions requises avant implémentation

| # | Question | Options | Impact |
|---|----------|---------|--------|
| 1 | **iOS dans le périmètre ?** | Oui (`staticlib` en plus de `cdylib`) / Non | `Cargo.toml` + `crate-type`, dossier `ios/` |
| 2 | **`getDay` secondaires** | Retourner `registry_indices` bruts (actuel) / Résoudre automatiquement en `FeastEntryData` | N appels `kal_read_feast` supplémentaires |
| 3 | **`build_id` mismatch** | Exception (recommandé) / Code d'erreur retourné | API de `load()` |
| 4 | **Workspace `Cargo.toml` racine** | Ajouter `./crates/liturgical-calendar-flutter` aux membres | Build intégré |

---

## 11. Ce qui a changé par rapport à la spec v1

| Spec v1 | Spec v2 |
|---------|---------|
| `CalendarEntry (8 octets)` — type v4 inexistant en v5 | `TimelineEntry (8 octets)` + `FeastEntry (4 octets)` |
| `Header (64 octets)` | `Header (80 octets)` — assert statique dans `header.rs` |
| `kal_flutter_get_entry` retournant `CalendarEntryFFI` par valeur | `kal_read_entry` → `*mut KalTimelineEntry` (pattern `out`) |
| Aucune exposition de `kal_read_feast` | `kal_read_feast` directement disponible dans la cdylib |
| 4 shims `kal_flutter_*` redondants avec le Core | 0 shim — 1 seule fonction nouvelle : `kal_lits_get_label` |
| Buffers statiques internes Rust | Stateless strict — Dart propriétaire de toute la mémoire |
| Crate à la racine | `./crates/liturgical-calendar-flutter/` |
