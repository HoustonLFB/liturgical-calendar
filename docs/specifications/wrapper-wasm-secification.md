# Session : `liturgical-calendar-wasm`

## Contexte projet

`liturgical-calendar` — pipeline AOT ECS/DOD. Rust toolchain 1.93.1.

Architecture en trois couches :

- **Core** (`liturgical-calendar-core`) : `no_std`, `no_alloc`, API C native. Lit les buffers `.kald` (topologie) et `.lits` (labels i18n) en O(1) via offsets.
- **Forge** (`liturgical-calendar-forge`) : compilateur AOT, produit les artefacts binaires.
- **Bridge** (`liturgical-calendar-wasm`) : crate de liaison à créer — expose l'API Core au JS via WASM.

Layout binaire `.kald` : magic `KALD`, header 64 octets, Entry Table `entry_count × sizeof(CalendarEntry)`. Chaque `CalendarEntry` expose `feast_id`, `flags`, `period`, `nature`, `color`, `precedence`, `secondary_index`, `secondary_count`, `vesperae_i`.

Layout binaire `.lits` : magic `LITS`, header 32 octets dont `kald_build_id` (octets 12–19 = `kald_checksum[..8]`), Entry Table `entry_count × 14 octets` (`feast_id`, `from`, `to`, `label_offset`, `annotation_offset`), String Pool UTF-8 null-terminé.

---

## Objectif de la session

Implémenter `crates/liturgical-calendar-wasm/` selon la topologie validée :

```
crates/liturgical-calendar-wasm/
├── Cargo.toml
└── src/
    └── lib.rs

www/
├── index.html
└── app.js
```

---

## Décisions architecturales actées

### Bridge (`src/lib.rs`)

**Cible** : `wasm32-unknown-unknown`, type `cdylib`. Pas de `wee_alloc` (abandonné). Allocateur statique minimal — le JS n'alloue que deux buffers au démarrage.

**Pattern d'allocation en deux temps** (évite les offsets arbitraires côté JS) :

```rust
// Phase 1 — bridge alloue, retourne le pointeur
#[no_mangle] pub extern "C" fn kal_wasm_alloc_kald(len: u32) -> u32;
#[no_mangle] pub extern "C" fn kal_wasm_alloc_lits(len: u32) -> u32;

// Phase 2 — JS écrit dans WebAssembly.Memory[ptr..ptr+len], puis commit
#[no_mangle] pub extern "C" fn kal_wasm_commit_kald() -> i32;  // valide header
#[no_mangle] pub extern "C" fn kal_wasm_commit_lits() -> i32;  // vérifie build_id
```

**État d'initialisation explicite** — enum statique, pas d'heap :

```rust
enum BridgeState { Uninitialized, KaldLoaded, Ready }
```

Tout appel à `kal_read_entry` ou `kal_wasm_get_label` depuis `!= Ready` retourne un code d'erreur dédié.

**Frontière WASM pour `WasmStringView`** — multi-valeur non supporté en MVP, pattern out-static :

```rust
static mut LABEL_VIEW: (u32, u32) = (0, 0); // ptr, len

#[no_mangle] pub extern "C" fn kal_wasm_get_label(year: u16, doy: u16) -> i32;
#[no_mangle] pub extern "C" fn kal_wasm_label_ptr() -> u32;
#[no_mangle] pub extern "C" fn kal_wasm_label_len() -> u32;
```

`kal_wasm_get_label` écrit dans `LABEL_VIEW` et retourne 1 si OK, 0 si absent. Le JS lit ensuite `label_ptr` + `label_len` et décode via `TextDecoder` — zero-copy préservé, aucune allocation côté bridge.

### Application (`www/app.js`)

Orchestration JS brute, sans framework ni `wasm-bindgen` :

1. `fetch()` → `.wasm`, `.kald`, `.lits`
2. `kal_wasm_alloc_kald(size)` → ptr ; copie dans `Uint8Array(memory.buffer, ptr, size)`
3. `kal_wasm_commit_kald()` → validation header
4. Même séquence pour `.lits`
5. `kal_read_entry(ptr_kald, len_kald, year, doy, ptr_out)` → lecture entrée
6. `kal_wasm_get_label(year, doy)` → `TextDecoder` sur `[label_ptr, label_ptr + label_len)`

---

## Signatures FFI Core existantes à wrapper

À vérifier dans `ffi.rs` avant d'écrire le bridge :

```c
i32 kal_validate_header(const uint8_t *buf, uintptr_t len, uint32_t *out_variant_id);
i32 kal_read_entry(const uint8_t *buf, uintptr_t len, uint16_t year, uint16_t doy, CalendarEntry *out);
i32 kal_read_secondary(const uint8_t *buf, uintptr_t len, uint32_t index, uint8_t count, uint16_t *out_ids, uint8_t capacity);
```

`LitsProvider` (dans `lits_provider.rs`) résout les labels — wrapper `kal_wasm_get_label` s'appuie dessus.

---

## Fichiers à fournir en contexte

**Indispensables :**

- `liturgical-calendar-core/src/ffi.rs`
- `liturgical-calendar-core/src/lits_provider.rs`
- `liturgical-calendar-core/src/entry.rs`
- `liturgical-calendar-core/src/types.rs`
- `liturgical-calendar-core/Cargo.toml`
- `Cargo.toml` (racine workspace — pour ajouter le membre `crates/liturgical-calendar-wasm`)

**Utiles :**

- `liturgical-calendar-core/src/header.rs` (layout header `.kald` — constantes magic, offsets)
- `liturgical-calendar-core/cbindgen.toml` (conventions de nommage C existantes)

---

## Vérification attendue après implémentation

```shell
# Build WASM
cargo build -p liturgical-calendar-wasm --target wasm32-unknown-unknown --release

# Serveur local
python3 -m http.server 8080 --directory www/

# Navigateur : http://localhost:8080
# → label du jour courant affiché sans backend, sans allocation JS de chaîne
```
