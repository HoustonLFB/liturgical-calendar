# liturgical-calendar-wasm

Bridge WASM pour [`liturgical-calendar-core`](../liturgical-calendar-core). Expose l'API C native du Core au JavaScript via `WebAssembly`, sans `wasm-bindgen`, sans allocateur heap, sans framework.

---

## Présentation

Ce crate est la couche de liaison entre le runtime liturgique et le navigateur. Il compile en un binaire `.wasm` autonome que le JS charge et pilote directement via l'API `WebAssembly` standard.

**Contraintes architecturales :**

- Cible `wasm32-unknown-unknown`, type `cdylib`
- `no_std` — aucune dépendance à la bibliothèque standard
- Zéro allocation heap — buffers statiques BSS, zéro-init
- Zéro copie pour la résolution de labels (pattern out-static)
- Pas de glue générée — les exports WASM sont des fonctions C directes

**Ce crate ne fait pas partie du build workspace par défaut.** Il est exclu de `default-members` dans le `Cargo.toml` racine et doit être compilé explicitement avec une cible wasm32.

---

## Prérequis

### Toolchain Rust

```sh
rustup target add wasm32-unknown-unknown
```

Vérifier que la cible est installée :

```sh
rustup target list --installed | grep wasm32
# wasm32-unknown-unknown
```

### Artefacts binaires

Le bridge nécessite deux fichiers produits par `liturgical-calendar-forge` :

| Fichier         | Rôle                                                  |
| --------------- | ----------------------------------------------------- |
| `calendar.kald` | Table calendaire compilée (entrées + pool secondaire) |
| `calendar.lits` | Labels i18n UTF-8 (liés au `.kald` par `build_id`)    |

Ces fichiers doivent être placés dans `www/` avant de lancer le serveur.

---

## Configuration workspace

Le fichier `.cargo/config.toml` à la racine du workspace définit les alias de build et de publication pour ce crate :

```toml
# .cargo/config.toml  (racine du workspace)
[alias]
build-wasm   = "build -p liturgical-calendar-wasm --target wasm32-unknown-unknown --profile wasm-release"
publish-wasm = "publish -p liturgical-calendar-wasm --target wasm32-unknown-unknown"
```

Le profil `wasm-release` est déclaré dans le `Cargo.toml` racine :

```toml
[profile.wasm-release]
inherits      = "release"
opt-level     = "z"
lto           = true
codegen-units = 1
strip         = true
```

---

## Build

### Profil de développement

```sh
cargo build -p liturgical-calendar-wasm --target wasm32-unknown-unknown
```

Sortie : `target/wasm32-unknown-unknown/debug/liturgical_calendar_wasm.wasm`

### Profil de production (optimisé taille)

```sh
cargo build-wasm
```

Sortie : `target/wasm32-unknown-unknown/wasm-release/liturgical_calendar_wasm.wasm`

### Déploiement dans `www/`

```sh
cp target/wasm32-unknown-unknown/wasm-release/liturgical_calendar_wasm.wasm www/
```

### Publication sur crates.io

La vérification de `cargo publish` compile le crate — la cible doit être spécifiée explicitement, sans quoi le build host échoue (`no_std` sans panic handler natif).

```sh
cargo publish-wasm
```

---

## Lancement

Le serveur de développement `www/server.py` gère la réécriture SPA (toute requête vers un chemin non-fichier sert `index.html`) et silencieux sur les ressources statiques.

```sh
python3 www/server.py          # port 8080 par défaut
python3 www/server.py 9000     # port personnalisé
```

### Routes supportées

| URL                              | Vue                                        |
| -------------------------------- | ------------------------------------------ |
| `http://0.0.0.0:8080/`          | Jour courant                               |
| `http://0.0.0.0:8080/2026`      | Tableau annuel — 366 jours                 |
| `http://0.0.0.0:8080/2026/12/25`| Détail journalier — tous les champs        |
| `http://0.0.0.0:8080/#2026`     | Idem année — compatible tout hébergeur statique |
| `http://0.0.0.0:8080/#2026/12/25`| Idem jour — compatible tout hébergeur statique |

Le routage par hash (`#`) ne requiert aucune configuration serveur et fonctionne sur tout hébergeur de fichiers statiques (GitHub Pages, Netlify, S3…). Le routage par chemin requiert la réécriture SPA de `server.py` en développement, ou un fichier `_redirects` / `vercel.json` en production.

> `python3 -m http.server` ne suffit pas : il ne gère pas la réécriture SPA et bloque `fetch()` depuis `file://`.

---

## Layout binaire `.kald` — convention doy

La Forge réserve **366 créneaux fixes par année**, quelle que soit la bissextilité. Le slot 59 est toujours celui du 29 février : Padding Entry (`primary_id = 0`) pour les années non bissextiles, célébration réelle pour les années bissextiles. Tous les jours à partir du 1er mars ont donc un `doy` constant d'une année à l'autre.

| Date        | doy |
| ----------- | --: |
| 1er janvier |   0 |
| 28 février  |  58 |
| 29 février  |  59 |
| 1er mars    |  60 |
| 25 décembre | 359 |

---

## API WASM exportée

Toutes les fonctions suivent la convention d'appel C et sont appelées via `instance.exports.*`. Le bridge impose un protocole séquentiel strict — tout appel de lecture depuis un état incorrect retourne `KAL_ERR_NOT_READY` (-20).

### État du bridge

```
Uninitialized
    │  kal_wasm_alloc_kald(len) → ptr
    │  [JS écrit dans WebAssembly.Memory]
    │  kal_wasm_commit_kald()   → 0 si OK
    ▼
KaldLoaded
    │  kal_wasm_alloc_lits(len) → ptr
    │  [JS écrit dans WebAssembly.Memory]
    │  kal_wasm_commit_lits()   → 0 si OK
    ▼
Ready  ←  toutes les fonctions de lecture disponibles
```

### Allocation (phase 1)

```c
// Retourne le pointeur WASM destination pour le buffer .kald.
// Retourne 0 si len > KALD_CAP (2 MB).
uint32_t kal_wasm_alloc_kald(uint32_t len);

// Retourne le pointeur WASM destination pour le buffer .lits.
// Retourne 0 si len > LITS_CAP (512 KB).
uint32_t kal_wasm_alloc_lits(uint32_t len);
```

### Commit (phase 2)

```c
// Valide le header .kald (magic, version, discriminant de layout, SHA-256).
// Retourne 0 (OK) ou un code d'erreur Core négatif.
int32_t kal_wasm_commit_kald(void);

// Vérifie build_id (kald_checksum[..8] == lits_header[12..20])
// et valide le header .lits.
// Retourne 0 (OK) ou un code d'erreur négatif.
int32_t kal_wasm_commit_lits(void);
```

### Lecture d'entrée — pattern out-static (ENTRY_BUF)

```c
// Lit l'entrée CalendarEntry (8 octets) pour (year, doy) dans ENTRY_BUF interne.
// doy : 0-based, layout fixe 366 slots/an (voir convention doy ci-dessus).
// Retourne 0 (OK) ou un code d'erreur négatif.
int32_t  kal_wasm_read_day(uint16_t year, uint16_t doy);

// Pointeur WASM vers ENTRY_BUF (8 octets, little-endian).
// Valide après kal_wasm_read_day.
uint32_t kal_wasm_entry_ptr(void);
```

Layout de `ENTRY_BUF` (miroir de `CalendarEntry`) :

| Offset | Champ             | Type   | Décodage                                    |
| -----: | ----------------- | ------ | ------------------------------------------- |
|    0–1 | `primary_id`      | u16 LE | 0 = Padding Entry (aucune fête)             |
|    2–3 | `secondary_index` | u16 LE | Index dans le Secondary Pool                |
|    4–5 | `flags`           | u16 LE | Voir décodage ci-dessous                    |
|      6 | `secondary_count` | u8     | Nombre de fêtes secondaires                 |
|      7 | `_reserved`       | u8     | Ignoré                                      |

Décodage des `flags` :

| Bits   | Champ              | Valeurs                              |
| ------ | ------------------ | ------------------------------------ |
| [3:0]  | `Precedence`       | 0–12 (voir lookup table `PRECEDENCE`)|
| [7:4]  | `Color`            | 0–5 (Albus, Rubeus, Viridis…)        |
| [10:8] | `LiturgicalPeriod` | 0–6                                  |
| [13:11]| `Nature`           | 0–5 (Sollemnitas, Festum…)           |
| [14]   | `has_vesperae_i`   | 1 = Premières Vêpres ce soir         |
| [15]   | `has_vigilia`      | 1 = Messe de Vigile propre           |

### Fêtes secondaires — pattern out-static (SECONDARY_BUF)

```c
// Remplit SECONDARY_BUF avec les IDs secondaires à partir de secondary_index.
// index : secondary_index lu depuis ENTRY_BUF[2..4].
// count : secondary_count lu depuis ENTRY_BUF[6].
// Retourne le nombre d'IDs écrits (≥ 0) ou code d'erreur négatif.
int32_t  kal_wasm_read_secondary(uint16_t index, uint8_t count);

// Pointeur WASM vers SECONDARY_BUF (tableau de uint16_t LE).
// Valide après kal_wasm_read_secondary.
uint32_t kal_wasm_secondary_ptr(void);
```

### Résolution de label et annotation — pattern out-static

WASM MVP ne supporte pas le retour multi-valeur. Les résultats sont écrits dans des statics internes lus via accesseurs.

```c
// Résout label + annotation pour (year, doy) — lit d'abord l'entrée.
// Retourne : 1 = trouvé, 0 = absent (Padding Entry ou corpus), < 0 = erreur.
int32_t  kal_wasm_get_label(uint16_t year, uint16_t doy);

// Résout label + annotation directement par feast_id (fêtes secondaires).
// Retourne : 1 = trouvé, 0 = absent, < 0 = erreur.
int32_t  kal_wasm_get_label_by_id(uint16_t feast_id, uint16_t year);

// Pointeur WASM vers le label UTF-8 (sans null terminal).
uint32_t kal_wasm_label_ptr(void);
uint32_t kal_wasm_label_len(void);

// Pointeur WASM vers l'annotation UTF-8. ANNOTATION_LEN == 0 si absente.
// L'annotation peut contenir du Markdown inline (*italic*).
uint32_t kal_wasm_annotation_ptr(void);
uint32_t kal_wasm_annotation_len(void);
```

### Codes d'erreur

| Code | Constante                   | Origine | Signification                                          |
| ---: | --------------------------- | ------- | ------------------------------------------------------ |
|    0 | `KAL_ENGINE_OK`             | Core    | Succès                                                 |
|   -1 | `KAL_ERR_NULL_PTR`          | Core    | Pointeur null passé à une fonction FFI                 |
|   -2 | `KAL_ERR_BUF_TOO_SMALL`     | Core    | Buffer trop court pour contenir un header valide       |
|   -3 | `KAL_ERR_MAGIC`             | Core    | Magic bytes invalides                                  |
|   -4 | `KAL_ERR_VERSION`           | Core    | Version du format non supportée                        |
|   -5 | `KAL_ERR_CHECKSUM`          | Core    | SHA-256 incorrect — corruption du payload              |
|   -6 | `KAL_ERR_FILE_SIZE`         | Core    | Taille du buffer incohérente avec les métadonnées      |
|   -8 | `KAL_ERR_INDEX_OOB`         | Core    | `(year, doy)` hors de la plage couverte                |
|  -10 | `KAL_ERR_SCHEMA`            | Core    | Discriminant de layout incompatible (dérive de schéma) |
|  -20 | `KAL_ERR_NOT_READY`         | Bridge  | Bridge non dans l'état requis                          |
|  -21 | `KAL_ERR_BUF_OVERFLOW`      | Bridge  | Taille demandée > capacité statique                    |
|  -22 | `KAL_ERR_BUILD_ID_MISMATCH` | Bridge  | `.kald` et `.lits` issus de builds différents          |
|  -23 | `KAL_ERR_LITS_INVALID`      | Bridge  | Header `.lits` invalide                                |

---

## Intégration JS

Séquence complète d'initialisation et de lecture :

```js
const { instance } = await WebAssembly.instantiateStreaming(
    fetch("liturgical_calendar_wasm.wasm"), {}
);
const { exports } = instance;
const memory = exports.memory;

// — Commit .kald —
const kaldBuf = await fetch("calendar.kald").then(r => r.arrayBuffer());
const kaldPtr = exports.kal_wasm_alloc_kald(kaldBuf.byteLength);
new Uint8Array(memory.buffer, kaldPtr, kaldBuf.byteLength).set(new Uint8Array(kaldBuf));
exports.kal_wasm_commit_kald(); // 0 = OK

// — Commit .lits —
const litsBuf = await fetch("calendar.lits").then(r => r.arrayBuffer());
const litsPtr = exports.kal_wasm_alloc_lits(litsBuf.byteLength);
new Uint8Array(memory.buffer, litsPtr, litsBuf.byteLength).set(new Uint8Array(litsBuf));
exports.kal_wasm_commit_lits(); // 0 = OK

// — Lecture d'une entrée —
exports.kal_wasm_read_day(2026, 359); // 25 décembre 2026
const view           = new DataView(memory.buffer, exports.kal_wasm_entry_ptr(), 8);
const primaryId      = view.getUint16(0, true);
const secondaryIndex = view.getUint16(2, true);
const flags          = view.getUint16(4, true);
const secondaryCount = view.getUint8(6);

// — Label + annotation fête principale —
const decoder = new TextDecoder("utf-8");
exports.kal_wasm_get_label(2026, 359);
const label = decoder.decode(new Uint8Array(
    memory.buffer, exports.kal_wasm_label_ptr(), exports.kal_wasm_label_len()
));
// annotation : présente si kal_wasm_annotation_len() > 0

// — Fêtes secondaires —
const n = exports.kal_wasm_read_secondary(secondaryIndex, secondaryCount);
const secView = new DataView(memory.buffer, exports.kal_wasm_secondary_ptr(), n * 2);
for (let i = 0; i < n; i++) {
    const secId = secView.getUint16(i * 2, true);
    exports.kal_wasm_get_label_by_id(secId, 2026);
    const secLabel = decoder.decode(new Uint8Array(
        memory.buffer, exports.kal_wasm_label_ptr(), exports.kal_wasm_label_len()
    ));
}
```

Tous les labels sont décodés zero-copy : `TextDecoder` opère directement sur `WebAssembly.Memory` sans copie intermédiaire.

---

## Limites des buffers statiques

| Buffer          | Capacité | Dimensionnement                                  |
| --------------- | -------- | ------------------------------------------------ |
| `KALD_BUF`      | 2 MB     | 431 ans × 366 jours × 8 octets ≈ 1.26 MB + pool |
| `LITS_BUF`      | 512 KB   | Labels UTF-8 multi-langues                       |
| `ENTRY_BUF`     | 8 octets | Une `CalendarEntry`                              |
| `SECONDARY_BUF` | 32 × u16 | IDs secondaires par jour (max 32)                |

Si un artefact dépasse `KALD_CAP` ou `LITS_CAP`, `kal_wasm_alloc_*` retourne `0`. Les constantes sont ajustables dans `src/lib.rs` avant recompilation.

---

## Structure

```
crates/liturgical-calendar-wasm/
├── Cargo.toml     — cdylib, dépendance Core
└── src/
    └── lib.rs     — bridge complet

www/
├── index.html                       — page hôte (vues annuelle + journalière)
├── app.js                           — routeur et orchestration JS
├── server.py                        — serveur de développement SPA
├── liturgical_calendar_wasm.wasm    — binaire compilé (copié depuis target/)
├── calendar.kald                    — artefact Forge
└── calendar.lits                    — artefact Forge
```
