# liturgical_calendar

Flutter plugin pour l'Engine de calendrier liturgique.  
Wrapper FFI sans allocation Rust — buffers `.kald` et `.lits` gérés côté Dart.

## Prérequis

- Rust toolchain + `cargo` disponibles au moment du build
- Cibles cross-compilation installées selon la plateforme cible :
  - Android : `cargo install cargo-ndk` + NDK configuré
  - Linux : `x86_64-unknown-linux-gnu` (natif)
  - Windows : `x86_64-pc-windows-gnu` ou MSVC
  - macOS : `x86_64-apple-darwin` / `aarch64-apple-darwin`

## Compilation de la bibliothèque native

```bash
# Depuis la racine du workspace Cargo :

# Linux / macOS (développement)
cargo build --release -p liturgical-calendar-flutter

# Android (arm64)
cargo ndk -t arm64-v8a build --release -p liturgical-calendar-flutter

# Windows (MSVC)
cargo build --release --target x86_64-pc-windows-msvc -p liturgical-calendar-flutter
```

La bibliothèque compilée (`libkal_flutter.so`, `.dll`, `.dylib`) doit être
copiée dans le dossier attendu par Flutter pour la plateforme cible
(ex. `android/src/main/jniLibs/arm64-v8a/libkal_flutter.so`).

## Usage

```dart
import 'package:liturgical_calendar_flutter/liturgical_calendar_flutter.dart';
import 'package:flutter/services.dart' show rootBundle;

// Charger les données depuis les assets Flutter.
Future<LiturgicalCalendar> openCalendar() async {
  final kald = (await rootBundle.load('assets/calendar.kald')).buffer.asUint8List();
  final lits = (await rootBundle.load('assets/calendar.lits')).buffer.asUint8List();

  final cal = LiturgicalCalendar();
  cal.load(kald, lits); // valide SHA-256 + build_id
  return cal;
}

// Interroger un jour.
void printDay(LiturgicalCalendar cal, int year, int month, int day) {
  final data = cal.getDay(year, month, day);
  if (data == null) return; // hors plage

  if (data.timeline.isPadding) {
    print('Feria (aucune fête)');
    return;
  }

  final feast = data.primary!;
  final label = cal.getLabel(feast.feastId, year);

  print('${label?.label} — ${feast.color} — ${feast.precedence}');

  // Célébrations secondaires.
  for (final idx in data.secondaryIndices) {
    final sec = cal.getFeast(idx);
    final secLabel = cal.getLabel(sec!.feastId, year);
    print('  + ${secLabel?.label}');
  }
}

// Libérer en fin de session.
cal.dispose();
```

## API

### `LiturgicalCalendar`

| Méthode                                            | Description                                                                                                        |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `load(kald, lits)`                                 | Charge et valide les deux buffers. Lève [KalValidationException] ou [KalBuildIdMismatchException] en cas d'erreur. |
| `getDay(year, month, day)`                         | Retourne [DayData] ou `null` si hors plage.                                                                        |
| `getFeast(registryIndex)`                          | Résout un registry_index en [FeastEntryData].                                                                      |
| `getLabel(feastId, year)`                          | Retourne [LitsEntryData] (label + annotation).                                                                     |
| `scanFlags(yearFrom, yearTo, flagMask, flagValue)` | Scan de flags sur une plage d'années.                                                                              |
| `dispose()`                                        | Libère les buffers natifs.                                                                                         |
| `toDoy(year, month, day)`                          | Convertit une date en day-of-year 0-based (utilitaire).                                                            |

### Types de retour

- **[DayData]** : `timeline` (TimelineEntryData) + `primary` (FeastEntryData?) + `secondaryIndices` (List\<int\>)
- **[FeastEntryData]** : `feastId`, `precedence`, `color`, `period`, `nature`, `hasVigilMass`
- **[LitsEntryData]** : `label` (String), `annotation` (String?)

### Conversions `doy`

Le format `.kald` utilise un `doy` 0-based sur 366 slots par année civile.
Le slot 59 est le Padding Feb29 — toujours présent en mémoire, même les
années non-bissextiles. `toDoy(year, 3, 1)` retourne 60 (pas 59).

## Tests

```bash
flutter test
```

Les tests génèrent leurs propres fixtures en mémoire (SHA-256 calculé via
`package:crypto`) — aucun fichier binaire requis. La bibliothèque native
compilée doit être accessible depuis `LD_LIBRARY_PATH` (Linux) /
`DYLD_LIBRARY_PATH` (macOS) / `PATH` (Windows).

## Structure

```
crates/
├── liturgical-calendar-flutter/   ← crate Rust (cdylib)
│   ├── Cargo.toml
│   └── src/lib.rs                 ← kal_lits_get_label uniquement
│
└── liturgical_calendar/           ← package Flutter (ce répertoire)
    ├── pubspec.yaml
    └── lib/src/
        ├── bindings.dart          ← DynamicLibrary + typedefs FFI
        ├── ffi_structs.dart       ← KalTimelineEntry, KalFeastEntry, KalHeader
        ├── calendar.dart          ← LiturgicalCalendar
        ├── types.dart             ← exceptions, enums, data classes
        └── error_codes.dart       ← constantes KAL_ERR_*
```

## Notes

- **iOS** non supporté dans cette version (le format `cdylib` nécessite une
  configuration `staticlib` + Xcode spécifique).
- **Pas de `flutter_rust_bridge`** — liaison FFI directe, zéro overhead de
  sérialisation.
- **Thread safety** : les fonctions FFI du Core sont stateless. La classe
  `LiturgicalCalendar` elle-même n'est pas thread-safe pour `load`/`dispose`.
