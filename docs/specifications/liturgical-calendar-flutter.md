Maintenant voici une nouvelle proposition pour un nouveau crate dédié à Flutter ; il faut évaluer cette proposition que j'ai préparé avec DeepSeek et Gemini :

---

# Création d'un wrapper pour Flutter

Expertise requise en Rust, Dart, Flutter et interopérabilité native via dart:ffi.

Nous allons créer le crate `liturgical-calendar-flutter` et le package Flutter associé, qui permet d’utiliser l’Engine liturgique existant (`liturgical-calendar-core`) dans une application Flutter native (Android, Linux, Windows, macOS).

Contexte technique :
- L’Engine (crate `liturgical-calendar-core`) est no_std, no_alloc. Il expose des fonctions C-ABI dans `ffi.rs` :
  - `kal_validate_header(data, len, out_header) -> i32`
  - `kal_read_entry(data, len, year, doy, out_entry) -> i32`
  - `kal_read_secondary(data, len, secondary_index, secondary_count, out_ids, out_capacity) -> i32`
  - `kal_scan_flags(data, len, year_from, year_to, flag_mask, flag_value, out_indices, out_capacity, out_count) -> i32`
- Les constantes : `KAL_ENGINE_OK = 0`, codes d’erreur (-1 à -10, plus -22 pour build_id mismatch).
- Les types : `CalendarEntry` (8 octets), `Header` (64 octets), types canoniques (`Precedence`, `Color`, `Nature`, `LiturgicalPeriod`).
- Le format `.kald` impose 366 slots par an avec le slot 59 réservé au 29 février (Padding Entry si non bissextile). Le doy est 0‑based (0 = 1er janvier). La conversion `(year, month, day) → doy` doit utiliser la table fixe `MONTH_OFFSETS = [0,31,60,91,121,152,182,213,244,274,305,335]` pour éviter toute dépendance à la bissextilité.
- L’Engine attend de recevoir un buffer complet `.kald` et un buffer `.lits` chargés en mémoire (passés en pointeurs). L’appelant doit allouer la mémoire et copier les données. Dans WASM, nous avions `kal_wasm_alloc_kald`, `kal_wasm_commit_kald`, etc. Pour Flutter, nous allons exposer une gestion similaire, mais adaptée : le crate Rust contiendra des buffers statiques internes (ou une allocation dynamique simplifiée) pour que l’API Dart soit simple.

Objectif :
Créer un nouveau crate Rust `liturgical-calendar-flutter` qui enveloppe l’Engine avec des fonctions `extern "C"` dédiées, conçues pour être appelées depuis Dart via `dart:ffi`. Il doit fournir une API haut niveau : chargement des binaires, accès à une entrée par date, récupération du label/annotation depuis le `.lits`, scan de flags, et gestion d’erreur.

Puis, créer le package Flutter (`liturgical_calendar`) en Dart, qui :
- Charge la bibliothèque native (`libkal_flutter.so` / `kal_flutter.dll` / `libkal_flutter.dylib`) selon la plateforme.
- Définit les signatures FFI Dart.
- Fournit une classe `LiturgicalCalendar` avec :
  - `Future<void> load(String kaldAssetPath, String litsAssetPath)` : charge les fichiers depuis les assets, les copie en mémoire native.
  - `CalendarEntry? getEntry(int year, int month, int day)` : convertit en doy, appelle `kal_read_entry`, retourne l’entrée ou null si erreur.
  - `String? getLabel(int year, int month, int day)` : idem + résolution `.lits`.
  - `List<int> scanFlags(int yearFrom, int yearTo, ...)` etc.
- Gère proprement la mémoire (libération lors du hot restart / fermeture).
- Inclut des tests unitaires Dart avec des buffers `.kald`/`.lits` de test embarqués dans le package.

Contraintes :
- Le code Rust doit rester `no_std` si possible.
- Utiliser `dart:ffi` directement, pas de `flutter_rust_bridge` (pour une intégration plus légère).
- Fournir des exemples d’utilisation.
- Documenter chaque fonction.
- Le package Flutter doit être pub.dev ready (nom `liturgical_calendar`, version `0.1.0`, licence MIT).

Structure attendue du crate Rust :
```

liturgical-calendar-flutter/
├── Cargo.toml   (dépendance à liturgical-calendar-core)
├── src/
│   └── lib.rs   (fonctions extern "C", buffers statiques, wrapper)
└── build.rs     (éventuellement)

```

Structure attendue du package Flutter :
```

liturgical_calendar/
├── lib/
│   ├── liturgical_calendar.dart
│   └── src/
│       ├── native_bindings.dart  (signatures FFI)
│       ├── calendar.dart         (classe LiturgicalCalendar)
│       ├── types.dart            (CalendarEntry etc. convertis en Dart)
│       └── ...
├── test/
│   ├── calendar_test.dart
│   └── test_assets/              (petits .kald/.lits de test)
│       ├── test.kald
│       └── test.lits
├── pubspec.yaml
├── android/   (si nécessaire pour les JNI libs, mais le build sera manuel)
├── linux/
├── windows/
├── macos/
└── README.md

```

### Directives pour les fonctions Rust à exposer (Strictement Stateless & Zero-Copy) :

L'Engine Rust ne doit conserver AUCUN état global (ni Mutex, ni Vec). Dart est responsable du cycle de vie de la mémoire.

- `kal_flutter_validate(kald_bytes: *const u8, kald_len: usize) -> i32` : simple délégation à `kal_validate_header`.
- `kal_flutter_get_entry(kald_bytes: *const u8, kald_len: usize, year: u16, doy: u16) -> CalendarEntryFFI` : lit l'entrée depuis le pointeur fourni et la retourne par valeur.
- `kal_flutter_get_label(lits_bytes: *const u8, lits_len: usize, feast_id: u16, year: u16, out_label_ptr: *mut *const u8, out_label_len: *mut usize, out_annotation_ptr: *mut *const u8, out_annotation_len: *mut usize) -> i32` : utilise LitsProvider. Écrit les pointeurs absolus (pointant DANS lits_bytes) et les longueurs dans les paramètres out. AUCUNE allocation de chaîne, AUCUNE copie.
- `kal_flutter_scan_flags(...) -> i32` : scan direct sur le pointeur kald_bytes.
- (Interdit) : Aucune fonction `_free`, aucune allocation dynamique, aucune dépendance à `std`.

Côté Dart (Gestion mémoire) :
- Dart chargera les assets `.kald` et `.lits` en mémoire (ex: via `calloc` de ffi, ou en pinant des Uint8List).
- La classe `LiturgicalCalendar` stockera ces pointeurs (`Pointer<Uint8>`).
- À chaque appel (ex: `getEntry`), Dart passera ces pointeurs à la fonction C correspondante.
- Lors de la destruction de la classe (ou via un NativeFinalizer), Dart libérera lui-même la mémoire. Dart lira les labels en instanciant des chaînes via `pointer.cast<Utf8>().toDartString(length: len)` pour préserver le zero-copy natif.
