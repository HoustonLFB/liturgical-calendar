# liturgical-calendar-wasm

Bridge WASM pour [`liturgical-calendar-core`](../liturgical-calendar-core). Expose l'API C native du Core au JavaScript via `WebAssembly`, sans `wasm-bindgen`, sans allocateur heap, sans framework.

---

## Présentation

Ce crate est la couche de liaison entre le runtime liturgique et le navigateur. Il compile en un binaire `.wasm` autonome que le JS charge et pilote directement via l'API `WebAssembly` standard.

**Contraintes architecturales :**

- Cible `wasm32-unknown-unknown`, type `cdylib`
- `no_std` — aucune dépendance à la bibliothèque standard
- Zéro allocation heap — deux buffers statiques BSS, zéro-init
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

## Build

### Profil de développement

```sh
cargo build -p liturgical-calendar-wasm --target wasm32-unknown-unknown
```

Sortie : `target/wasm32-unknown-unknown/debug/liturgical_calendar_wasm.wasm`

### Profil de production (optimisé taille)

Le profil `wasm-release` est défini dans le `Cargo.toml` racine du workspace :

```sh
cargo build -p liturgical-calendar-wasm \
  --target wasm32-unknown-unknown \
  --profile wasm-release
```

Sortie : `target/wasm32-unknown-unknown/wasm-release/liturgical_calendar_wasm.wasm`

### Déploiement dans `www/`

```sh
cp target/wasm32-unknown-unknown/wasm-release/liturgical_calendar_wasm.wasm www/
```

---

## Lancement

```sh
python3 -m http.server 8080 --directory www/
```

Ouvrir `http://localhost:8080` — le label liturgique du jour s'affiche sans backend, sans allocation JS de chaîne.

> Le serveur doit servir les fichiers avec les Content-Types corrects. Python 3's `http.server` le fait automatiquement pour `.wasm` depuis Python 3.7.

---

## API WASM exportée

Toutes les fonctions sont exportées avec `#[unsafe(no_mangle)]` et suivent la convention d'appel C. Le JS les appelle via `instance.exports.*`.

### Initialisation en deux phases

Le bridge impose un protocole séquentiel strict. Tout appel de lecture depuis un état incorrect retourne `KAL_ERR_NOT_READY` (-20).

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
Ready
```

### Fonctions d'allocation

```c
// Retourne le pointeur WASM destination pour le buffer .kald.
// Retourne 0 si len > KALD_CAP (2 MB).
uint32_t kal_wasm_alloc_kald(uint32_t len);

// Retourne le pointeur WASM destination pour le buffer .lits.
// Retourne 0 si len > LITS_CAP (512 KB).
uint32_t kal_wasm_alloc_lits(uint32_t len);
```

### Fonctions de commit

```c
// Valide le header .kald (magic, version, discriminant de layout, SHA-256).
// Retourne 0 (OK) ou un code d'erreur Core négatif.
int32_t kal_wasm_commit_kald(void);

// Vérifie build_id et valide le header .lits.
// Retourne 0 (OK) ou un code d'erreur négatif.
int32_t kal_wasm_commit_lits(void);
```

### Lecture d'entrée

```c
// Lit l'entrée CalendarEntry (8 octets) pour (year, doy) dans out_entry.
// doy est 0-based (0 = 1er janvier).
// Retourne 0 (OK) ou un code d'erreur négatif.
int32_t kal_wasm_read_entry(uint16_t year, uint16_t doy, uint8_t *out_entry);
```

### Résolution de label (pattern out-static)

WASM MVP ne supporte pas le retour multi-valeur. Le label résolu est écrit dans deux statics internes ; le JS les lit via deux accesseurs dédiés immédiatement après l'appel.

```c
// Résout le label principal pour (year, doy).
// Retourne : 1 = trouvé, 0 = absent (Padding Entry ou corpus),
//            < 0 = code d'erreur.
int32_t kal_wasm_get_label(uint16_t year, uint16_t doy);

// Pointeur WASM vers le label (UTF-8, sans null terminal).
// Valide uniquement si kal_wasm_get_label a retourné 1.
uint32_t kal_wasm_label_ptr(void);

// Longueur en octets du label.
// Valide uniquement si kal_wasm_get_label a retourné 1.
uint32_t kal_wasm_label_len(void);
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

Séquence complète dans `www/app.js` :

```js
const { instance } = await WebAssembly.instantiateStreaming(fetch("liturgical_calendar_wasm.wasm"), {});
const { exports } = instance;
const memory = exports.memory;

// — Commit .kald —
const kaldBuf = await fetch("calendar.kald").then(r => r.arrayBuffer());
const kaldPtr = exports.kal_wasm_alloc_kald(kaldBuf.byteLength);
new Uint8Array(memory.buffer, kaldPtr, kaldBuf.byteLength).set(new Uint8Array(kaldBuf));
const rcKald = exports.kal_wasm_commit_kald(); // 0 = OK

// — Commit .lits —
const litsBuf = await fetch("calendar.lits").then(r => r.arrayBuffer());
const litsPtr = exports.kal_wasm_alloc_lits(litsBuf.byteLength);
new Uint8Array(memory.buffer, litsPtr, litsBuf.byteLength).set(new Uint8Array(litsBuf));
const rcLits = exports.kal_wasm_commit_lits(); // 0 = OK

// — Label du jour —
const year = new Date().getFullYear();
const doy  = /* jour 0-based dans l'année */;
const rc = exports.kal_wasm_get_label(year, doy);
if (rc === 1) {
  const ptr   = exports.kal_wasm_label_ptr();
  const len   = exports.kal_wasm_label_len();
  const label = new TextDecoder().decode(new Uint8Array(memory.buffer, ptr, len));
}
```

Le label est décodé zero-copy : `TextDecoder` opère directement sur `WebAssembly.Memory` sans copie intermédiaire.

---

## Limites des buffers statiques

| Buffer     | Capacité | Dimensionnement                                 |
| ---------- | -------- | ----------------------------------------------- |
| `KALD_BUF` | 2 MB     | 431 ans × 366 jours × 8 octets ≈ 1.26 MB + pool |
| `LITS_BUF` | 512 KB   | Labels UTF-8 multi-langues                      |

Si un artefact dépasse ces capacités, `kal_wasm_alloc_*` retourne `0`. Les constantes `KALD_CAP` / `LITS_CAP` dans `src/lib.rs` peuvent être ajustées avant recompilation.

---

## Structure du crate

```
crates/liturgical-calendar-wasm/
├── Cargo.toml       — cdylib, dépendance Core, pas de [profile] (géré au workspace)
└── src/
    └── lib.rs       — bridge complet : alloc, commit, lecture, résolution label
```

```
www/
├── index.html                      — page hôte minimale
├── app.js                          — orchestration JS (fetch, alloc, commit, decode)
├── liturgical_calendar_wasm.wasm   — binaire compilé (à copier depuis target/)
├── calendar.kald                   — artefact Forge
└── calendar.lits                   — artefact Forge
```
