# liturgical_calendar_flutter

Plugin Flutter pour l'Engine de calendrier liturgique romain.
Liaison FFI directe — sans `flutter_rust_bridge`, sans allocation Rust, sans sérialisation.

## Architecture

```
liturgical-calendar-core       (no_std, no_alloc)
        │
        └── liturgical-calendar-flutter  (cdylib, no_std, no_alloc)
                │
                └── libkal_flutter.so / .dll / .dylib
                        │
                        └── Dart FFI  (ce package)
```

La couche Dart possède tous les buffers. Rust ne conserve aucun état entre les appels.

## Prérequis

- Toolchain Rust + `cargo` disponibles au moment du build.
- Cibles cross-compilation installées selon la plateforme :
  - Android : `cargo install cargo-ndk` + NDK configuré.
  - Linux : `x86_64-unknown-linux-gnu` (natif).
  - Windows : `x86_64-pc-windows-gnu` ou MSVC.
  - macOS : `x86_64-apple-darwin` / `aarch64-apple-darwin`.

## Compilation de la bibliothèque native

```bash
# Depuis la racine du workspace Cargo :

# Linux / macOS (développement)
cargo build --release -p liturgical-calendar-flutter

# Android (arm64)
cargo ndk -t arm64-v8a build --release -p liturgical-calendar-flutter

# Windows (MSVC)
cargo build --release --target x86_64-pc-windows-msvc -p liturgical-calendar-flutter

# macOS universel (arm64 + x86_64)
cargo build --release --target aarch64-apple-darwin -p liturgical-calendar-flutter
cargo build --release --target x86_64-apple-darwin  -p liturgical-calendar-flutter
lipo -create -output libkal_flutter.dylib \
    target/aarch64-apple-darwin/release/libkal_flutter.dylib \
    target/x86_64-apple-darwin/release/libkal_flutter.dylib
```

La bibliothèque compilée (`libkal_flutter.so`, `.dll`, `.dylib`) doit être copiée
dans le dossier attendu par Flutter pour la plateforme cible
(ex : `android/src/main/jniLibs/arm64-v8a/libkal_flutter.so`).

## Usage

```dart
import 'package:liturgical_calendar_flutter/liturgical_calendar_flutter.dart';
import 'package:flutter/services.dart' show rootBundle;

// Charger les données depuis les assets Flutter.
Future<LiturgicalCalendar> ouvrirCalendrier() async {
  final kald = (await rootBundle.load('assets/calendar.kald')).buffer.asUint8List();
  final lits = (await rootBundle.load('assets/calendar.lits')).buffer.asUint8List();

  final cal = LiturgicalCalendar();
  cal.load(kald, lits); // valide SHA-256 + build_id
  return cal;
}

// Interroger un jour.
void afficherJour(LiturgicalCalendar cal, int annee, int mois, int jour) {
  final data = cal.getDay(annee, mois, jour);
  if (data == null) return; // hors plage

  if (data.timeline.isPadding) {
    // v6 : même sans fête propre, les primitives temporelles sont disponibles.
    final periode = data.timeline.liturgicalPeriod;
    final semaine = data.timeline.liturgicalWeek;
    // Construire le label côté Dart :
    // ex: "Feria II, Hebdomada $semaine Temporis Ordinarii"
    return;
  }

  final fete = data.primary!;
  final label = cal.getLabel(fete.feastId, annee);

  print('${label?.label} — ${fete.color} — ${fete.precedence}');

  // Célébrations secondaires.
  for (final idx in data.secondaryIndices) {
    final sec = cal.getFeast(idx);
    final labelSec = cal.getLabel(sec!.feastId, annee);
    print('  + ${labelSec?.label}');
  }
}

// Libérer en fin de session.
cal.dispose();
```

## API

### `LiturgicalCalendar`

| Méthode                                            | Description                                                                                                        |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `load(kald, lits)`                                 | Charge et valide les deux buffers. Lève `KalValidationException` ou `KalBuildIdMismatchException` en cas d'erreur. |
| `getDay(annee, mois, jour)`                        | Retourne `DayData` ou `null` si hors plage.                                                                        |
| `getFeast(registryIndex)`                          | Résout un registry_index en `FeastEntryData`.                                                                      |
| `getLabel(feastId, annee)`                         | Retourne `LitsEntryData` (label + annotation).                                                                     |
| `scanFlags(yearFrom, yearTo, flagMask, flagValue)` | Scan de flags sur une plage d'années.                                                                              |
| `dispose()`                                        | Libère les buffers natifs.                                                                                         |
| `toDoy(annee, mois, jour)`                         | Convertit une date en day-of-year 0-based (utilitaire statique).                                                   |

### Types de retour

**`DayData`** — agrège les trois niveaux d'information :

- `timeline` (`TimelineEntryData`) — données d'occurrence, toujours présent.
- `primary` (`FeastEntryData?`) — invariants de la fête principale. `null` si Padding Entry.
- `secondaryIndices` (`List<int>`) — registry_indices des célébrations secondaires.

**`TimelineEntryData`** — miroir de `TimelineEntry` v6 :

| Champ              | Type                | Description                                                        |
| ------------------ | ------------------- | ------------------------------------------------------------------ |
| `primaryIndex`     | `int`               | 0 = Padding Entry (aucune fête propre)                             |
| `secondaryOffset`  | `int`               | Offset dans le Secondary Pool                                      |
| `secondaryCount`   | `int`               | Nombre de fêtes secondaires                                        |
| `hasVesperaeI`     | `bool`              | Premières Vêpres ce soir pour le lendemain                         |
| `hasVigilia`       | `bool`              | Messe de Vigile propre ce soir pour le lendemain                   |
| `liturgicalPeriod` | `LiturgicalPeriod?` | Période liturgique courante — valide même pour les Padding Entries |
| `liturgicalWeek`   | `int`               | Ordinal de semaine dans la période (0 = N/A, 1–34)                 |
| `isPadding`        | `bool` (getter)     | `true` si aucune fête propre                                       |

**`FeastEntryData`** — invariants d'une fête (independants du jour) :

| Champ          | Type               | Description                               |
| -------------- | ------------------ | ----------------------------------------- |
| `feastId`      | `int`              | Clé de lookup `.lits`                     |
| `flagsRaw`     | `int`              | Valeur brute des flags                    |
| `precedence`   | `Precedence?`      | Rang de préséance (0–12)                  |
| `color`        | `LiturgicalColor?` | Couleur liturgique                        |
| `nature`       | `Nature?`          | Nature de la célébration                  |
| `hasVigilMass` | `bool`             | Messe de Vigile propre associée au corpus |

> **v6 :** `LiturgicalPeriod` n'est plus dans `FeastEntryData`. Elle est portée
> par `TimelineEntryData.liturgicalPeriod` (issue de `TimelineEntry.occurrenceFlags[4:2]`).

**`LitsEntryData`** — label et annotation depuis le `.lits` :

- `label` (`String`) — titre officiel de la fête.
- `annotation` (`String?`) — précision liturgique ou titre alternatif. `null` si absent.

### Convention `doy`

Le format `.kald` utilise un `doy` 0-based sur **366 slots fixes par année civile**.
Le slot 59 est le Padding Feb29 — réservé même les années non-bissextiles.
`toDoy(annee, 3, 1)` retourne toujours 60, quelle que soit la bissextilité.

| Date        | doy |
| ----------- | --: |
| 1er janvier |   0 |
| 28 février  |  58 |
| 29 février  |  59 |
| 1er mars    |  60 |
| 25 décembre | 359 |

## Codes d'erreur

| Code | Constante               | Signification                                 |
| ---: | ----------------------- | --------------------------------------------- |
|    0 | `kalEngineOk`           | Succès                                        |
|   -2 | `kalErrBufTooSmall`     | Buffer trop court                             |
|   -3 | `kalErrMagic`           | Signature invalide                            |
|   -4 | `kalErrVersion`         | Version non supportée                         |
|   -5 | `kalErrChecksum`        | SHA-256 incorrect                             |
|   -6 | `kalErrFileSize`        | Taille incohérente                            |
|   -7 | `kalErrIndexOob`        | Année, doy ou registry_index hors plage       |
|  -10 | `kalErrSchema`          | Discriminant de layout incompatible           |
|  -22 | `kalErrBuildIdMismatch` | `.kald` et `.lits` issus de builds différents |

## Tests

```bash
flutter test
```

Les tests génèrent leurs propres fixtures en mémoire (SHA-256 calculé via
`package:crypto`) — aucun fichier binaire requis. La bibliothèque native
compilée doit être accessible depuis `LD_LIBRARY_PATH` (Linux) /
`DYLD_LIBRARY_PATH` (macOS) / `PATH` (Windows).

## Compatibilité des formats

Requiert `.kald` v6 et `.lits` v1, produits par `liturgical-calendar-forge`.
La vérification de cohérence `build_id` (`kald.checksum[0..8] == lits[12..20]`)
est assurée par `load()` au chargement.

## Notes

- **iOS** non supporté dans cette version (le format `cdylib` nécessite une
  configuration `staticlib` + Xcode spécifique).
- **Pas de `flutter_rust_bridge`** — liaison FFI directe, zéro overhead de sérialisation.
- **Thread safety** : les fonctions FFI du Core sont stateless. `LiturgicalCalendar`
  lui-même n'est pas thread-safe pour `load`/`dispose`.

## Structure

```
crates/
├── liturgical-calendar-flutter/   ← crate Rust (cdylib)
│   ├── Cargo.toml
│   └── src/lib.rs                 ← kal_lits_get_label uniquement
│
packages/liturgical_calendar_flutter/   ← ce package
├── pubspec.yaml
└── lib/src/
    ├── bindings.dart              ← DynamicLibrary + typedefs FFI
    ├── ffi_structs.dart           ← KalTimelineEntry, KalFeastEntry, KalHeader
    ├── calendar.dart              ← LiturgicalCalendar
    ├── types.dart                 ← exceptions, enums, data classes
    └── error_codes.dart           ← constantes kalErr*
```
