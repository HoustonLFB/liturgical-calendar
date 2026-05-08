# Prompt de reprise — Session : `kald-v5-feast-registry`

## Contexte projet

`liturgical-calendar` — pipeline AOT ECS/DOD. Rust toolchain 1.97.0.

Architecture en trois couches :
- **Core** (`liturgical-calendar-core`) : `no_std`, `no_alloc`, API C native. Lit les buffers `.kald` et `.lits` en O(1).
- **Forge** (`liturgical-calendar-forge`) : compilateur AOT, produit les artefacts binaires.
- **Bridge** (`liturgical-calendar-wasm`) : crate de liaison — expose l'API Core au JS via WASM. En production, non concerné par cette session.

## Diagnostic ayant motivé cette session

Le layout `.kald` actuel (format v4) couple deux domaines distincts dans un seul slot temporel :

```
CalendarEntry (8 octets, AoS) {
    primary_id:      u16,  // identifiant de la fête principale
    secondary_index: u16,  // index dans le Secondary Pool
    flags:           u16,  // ← invariants de la fête (couleur, nature, précédence...)
    secondary_count: u8,
    _reserved:       u8,
}
```

**Problème 1 — Redondance** : les `flags` sont des invariants de la fête (définis dans le corpus YAML), pas de l'occurrence journalière. Pour une fête s'étendant sur plusieurs jours (octave, triduum), ils sont dupliqués à l'identique dans chaque slot temporel.

**Problème 2 — Fuite d'abstraction** : le Secondary Pool ne stocke que des `u16` feast_ids. Une fête rétrogradée en commémoraison perd son `CalendarEntry`, rendant ses invariants (couleur, nature, précédence) inaccessibles — sauf scan linéaire O(366), inacceptable.

## Cible architecturale : format `.kald` v5

Séparation stricte en deux segments :

### Segment 1 — Feast Registry (invariants, AOT)

Array dense `[FeastEntry; registry_count]` indexé par `registry_index` (u16, 0-based).

```
FeastEntry (4 octets) {
    feast_id: u16,   // identifiant corpus (pour vérification croisée .lits)
    flags:    u16,   // Precedence[3:0] | Color[7:4] | Period[10:8] | Nature[13:11]
}
```

Accès O(1) : `registry_base + registry_index * 4`.

**Point de décision tranché** : les feast_ids sont séquentiels à partir de `FEAST_ID_BASE = 4097` (source : `variant_registry.lock`). Le `registry_index` se calcule par soustraction pure :

```
registry_index = feast_id - FEAST_ID_BASE
```

Aucune table de remapping. Le Registry est un array dense de `registry_count` entrées. `FEAST_ID_BASE` est stocké dans le header v5 pour validation à l'ouverture. La Timeline et le Secondary Pool stockent `registry_index` (u16, 0-based), pas `feast_id`. Le `feast_id` reste dans `FeastEntry` comme identifiant externe pour la vérification croisée avec `.lits`.

### Segment 2 — Timeline (occurrences, 366 slots/an)

```
TimelineEntry (8 octets, stride conservé) {
    primary_index:    u16,  // registry_index de la fête principale (0 = Padding)
    secondary_offset: u16,  // offset dans le Secondary Pool
    occurrence_flags: u8,   // bit 0: vesperae_i, bit 1: vigilia
    secondary_count:  u8,
    _reserved:        u16,  // padding pour conserver stride=8 et alignement
}
```

Les `flags` de fête migrent vers le Registry. Les bits 14–15 du champ `flags` actuel (`has_vesperae_i`, `has_vigilia`) migrent vers `occurrence_flags` — données d'occurrence, pas invariants.

### Secondary Pool

Inchangé structurellement : tableau de `u16` (`registry_index`, non plus `feast_id` si remapping).

### Header v5 (extension du header v4, 80 octets)

```
[0..4]   magic:              b"KALD"
[4..6]   version:            u16 = 5
[6..8]   variant_id:         u16
[8..10]  epoch:              u16
[10..12] range:              u16
[12..16] entry_count:        u32   (Timeline slots)
[16..20] pool_offset:        u32
[20..24] pool_size:          u32
[24..28] registry_offset:    u32   ← nouveau
[28..32] registry_count:     u32   ← nouveau
[32..34] feast_id_base:      u16   ← nouveau (= 4097 en l'état du corpus)
[34..36] _reserved:          u16
[36..68] checksum:           [u8; 32]  SHA-256(Registry ∥ Timeline ∥ Pool)
[64..72] layout_discriminant:[u8; 8]
[72..80] _reserved:          [u8; 8]
```

Bump de 64 → 80 octets (padding absorbé — _reserved existant). Le `LAYOUT_DISCRIMINANT` doit être recalculé pour `TimelineEntry`.

## Fichiers clés à lire en début de session

**Forge — pipeline de compilation :**
- `liturgical-calendar-forge/src/packing.rs` — écriture binaire actuelle du `.kald`
- `liturgical-calendar-forge/src/registry.rs` — gestion du registre de fêtes
- `liturgical-calendar-forge/src/materialization.rs` — résolution des occurrences journalières
- `liturgical-calendar-forge/src/id_alloc.rs` — allocation des feast_ids

**Corpus :**
- `corpus/romanus/feast_registry.lock` — **lecture obligatoire** pour trancher la question de compacité des feast_ids

**Core — layout actuel à faire évoluer :**
- `liturgical-calendar-core/src/entry.rs` — `CalendarEntry`, `LAYOUT_DISCRIMINANT`
- `liturgical-calendar-core/src/header.rs` — `Header`, `validate_header`
- `liturgical-calendar-core/src/ffi.rs` — `kal_read_entry`, `kal_read_secondary`

## Périmètre de la session

### Dans le scope

1. **Forge** : refonte du pipeline de sérialisation en deux passes (collecte Registry → écriture segments).
2. **Core** : nouvelles structs `TimelineEntry` + `FeastEntry`, nouveau `Header` v5, mise à jour de `validate_header`, `kal_read_entry`, `kal_read_secondary`, ajout de `kal_read_feast`.
3. **Tests** : mise à jour des helpers `make_valid_kald`, nouveaux cas pour le Registry.
4. **Fuzz** : mise à jour des trois targets existants.

### Hors scope (session ultérieure)

- Bridge WASM (`liturgical-calendar-wasm`) — sera mis à jour après stabilisation du Core v5.
- Format `.lits` — non affecté.
- CLI (`liturgical-calendar`) — non affecté dans cette session.

## Nouvelle API Core attendue

```c
// Lit la TimelineEntry pour (year, doy).
i32 kal_read_entry(const uint8_t *buf, uintptr_t len,
                   uint16_t year, uint16_t doy,
                   TimelineEntry *out);

// Lit le FeastEntry pour registry_index depuis le Feast Registry.
// Accès O(1) : registry_offset + registry_index * sizeof(FeastEntry).
i32 kal_read_feast(const uint8_t *buf, uintptr_t len,
                   uint16_t registry_index,
                   FeastEntry *out);

// Inchangé structurellement — les u16 sont désormais des registry_index.
i32 kal_read_secondary(const uint8_t *buf, uintptr_t len,
                       uint16_t secondary_offset, uint8_t count,
                       uint16_t *out_indices, uint8_t capacity);
```

## Invariants à préserver

- Stride Timeline = 8 octets (conserver l'alignement naturel, le discriminant existant comme base de calcul).
- `no_std`, `no_alloc` dans le Core.
- Zéro régression sur les tests existants une fois migrés.
- Le `KALD_FORMAT_VERSION` passe de `4` à `5` — tous les artefacts `.kald` existants deviennent invalides et doivent être régénérés par la Forge.
