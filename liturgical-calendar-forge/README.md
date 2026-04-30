# liturgical-calendar-forge

Compilateur du calendrier liturgique catholique (Novus Ordo, 1969–2399).

Ingère un corpus de fichiers YAML décrivant le droit liturgique (fêtes, préséances, transferts), calcule l'intégralité du calendrier sur 431 années, et produit deux artefacts binaires statiques consommés par [`liturgical-calendar-core`](https://crates.io/crates/liturgical-calendar-core) :

- **`.kald`** — topologie calendaire, 157 746 entrées de 8 octets
- **`.lits`** — labels i18n indexés par FeastID et plage d'années, un fichier par langue

## Philosophie

Toute la complexité liturgique — calcul de Pâques, résolution des préséances, transferts de fêtes, fallback latin — est traitée **une fois pour toutes** à la compilation. L'Engine runtime ne connaît que des lectures d'offset. Une modification du corpus YAML (canonisation, nouvelle règle) se traduit par une recompilation Forge — jamais par une modification du code applicatif.

## CLI

### Installation

```bash
cargo install --path ./liturgical-calendar-forge
# ou depuis crates.io :
cargo install liturgical-calendar-forge
```

### `kal-forge` — Compilation

```bash
# Scope universel romain — .kald seul
kal-forge -s universale

# Scope universel — .kald + .lits (toutes les langues découvertes)
kal-forge -s universale -i

# Propre national
kal-forge -s nationalia/FR -i

# Tout le rite romain (tous les scopes non-DRAFT)
kal-forge -i

# Rite alternatif
kal-forge -r ambrosianus -s universale -i
```

| Court | Long               | Défaut        | Description                                              |
|-------|--------------------|---------------|----------------------------------------------------------|
| `-r`  | `--rite`           | `romanus`     | Rite à compiler                                          |
| `-s`  | `--scope`          | _(tous)_      | Scope cible. Si absent : compile tout le rite            |
| `-c`  | `--corpus`         | `./corpus`    | Racine du corpus YAML                                    |
| `-o`  | `--out`            | `./artifacts` | Répertoire de sortie                                     |
| `-i`  | `--i18n`           | _(désactivé)_ | Active la production des `.lits`                         |
| `-d`  | `--include-drafts` | _(désactivé)_ | Compile les scopes marqués `DRAFT`                       |

### `kal-read` — Inspection

```bash
# Entrée structurelle
kal-read --kald ./artifacts/romanus_universale.kald --year 2025 --doy 109

# Avec label et annotation
kal-read --kald ./artifacts/romanus_universale.kald \
         --lits ./artifacts/romanus_universale_la.lits \
         --year 2025 --doy 109
```

```
year=2025  doy=109
  feast_id    : 0x0001
  precedence  : TriduumSacrum (0)
  color       : Albus (0)
  period      : TempusPaschale (5)
  nature      : Sollemnitas (0)
  vesperae_i  : false
  vigilia     : false

  label       : Dominica Resurrectionis
  annotation  : —
```

## Pipeline de compilation

```
YAML corpus + dictionnaires i18n
  ↓ Étape 1    Rule Parsing — validations V1–V6, V-T1–T5, V-I1–I2
  ↓ Étape 2    Canonicalization — Pâques, ancres, DOY
  ↓ Étape 3    Conflict Resolution — préséance, transfers
  ↓ Étape 4    Materialization — CalendarEntry, Secondary Pool
  ↓ Étape 5    Vespers pass — bits vespéraux, vigiles
  ↓ Étape 6    Binary Packing — .kald + .lits par langue
```

Toute violation de validation est **fatale** — la Forge n'émet aucun binaire partiel.

## Format du Corpus YAML

```yaml
# corpus/romanus/universale/sanctorale/01/petri_et_pauli.yaml
version: 1
category: 1
date:
  month: 6
  day: 29
history:
  - from: 1969
    precedence: 5      # Solennité (YAML 1-based → interne 0-based)
    nature: sollemnitas
    color: rubeus
    has_vigil_mass: true
```

```yaml
# corpus/romanus/universale/i18n/la/petri_et_pauli.yaml
version: 1
history:
  - from: 1969
    label: "Ss. Petri et Pauli, apostolorum"
```

Le corpus complet et la documentation du schéma YAML sont disponibles dans le dépôt source.

## Stabilité des Identifiants

Deux fichiers lock versionnés garantissent la stabilité des FeastIDs et variant_ids entre compilations :

```
corpus/{rite}/feast_registry.lock     — FeastID par slug
corpus/{rite}/variant_registry.lock   — variant_id par scope
```

Un slug retiré du corpus est **tombstoné** — son ID ne sera jamais réalloué.

## Artefacts Produits

Nommage : chemin de scope aplati avec `_` comme séparateur.

| Scope                   | `.kald`                       | `.lits`                           |
|-------------------------|-------------------------------|-----------------------------------|
| `romanus/universale`    | `romanus_universale.kald`     | `romanus_universale_la.lits`      |
| `romanus/nationalia/FR` | `romanus_nationalia_FR.kald`  | `romanus_nationalia_FR_fr.lits`   |

## Utilisation Programmatique

```rust
use liturgical_calendar_forge::{compile, parsing::ingest_corpus, I18nConfig};

let registry  = ingest_corpus(&corpus_root)?;
let checksum  = compile(registry, &output_path, variant_id, None, &lock_path)?;

let build_id = u64::from_le_bytes(checksum[..8].try_into().unwrap());
println!("build_id = {build_id:#018x}");
```

## Dépendances

- [`liturgical-calendar-core`](https://crates.io/crates/liturgical-calendar-core) — types `CalendarEntry`, validation header
- `serde` + `serde_yml` — désérialisation YAML
- `sha2` — SHA-256 du Data Body
- `toml` — lecture/écriture des fichiers lock
- `clap` — interface CLI
