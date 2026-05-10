# kal-forge — Mode d'emploi

`kal-forge` compile le corpus YAML liturgique en artefacts binaires :

- **`.kald`** — calendrier structurel (431 ans, 1969–2399)
- **`.lits`** — labels i18n couplés au `.kald` (un fichier par langue)

---

## Prérequis

- Rust toolchain ≥ 1.77
- Corpus YAML complet dans `corpus/`
- `feast_registry.lock` à jour dans `corpus/{rite}/`

```bash
cargo build -p liturgical-calendar-forge --release
```

L'exécutable se trouve dans `target/release/kal-forge`.

---

## Commandes

### Produire un `.kald` seul

```bash
cargo run -p liturgical-calendar-forge --bin kal-forge -- \
    --rite romanus \
    --scope universale \
    --corpus ./corpus \
    --out ./artifacts
```

Produit : `artifacts/romanus_universale.kald`

### Produire un `.kald` + les `.lits`

```bash
cargo run -p liturgical-calendar-forge --bin kal-forge -- \
    --rite romanus \
    --scope universale \
    --corpus ./corpus \
    --out ./artifacts \
    --i18n
```

Produit :

```
artifacts/
  romanus_universale.kald
  romanus_universale_la.lits
  romanus_universale_fr.lits   ← si fr/ existe dans la hiérarchie i18n
```

### Compiler un scope national

```bash
cargo run -p liturgical-calendar-forge --bin kal-forge -- \
    --rite romanus \
    --scope nationalia/FR \
    --corpus ./corpus \
    --out ./artifacts \
    --i18n
```

Produit : `artifacts/romanus_nationalia_FR.kald` + `.lits` associés.

---

## Arguments

| Argument           | Défaut        | Description                        |
| ------------------ | ------------- | ---------------------------------- |
| `--rite`           | `romanus`     | Rite à compiler                    |
| `--scope`          | `universale`  | Scope dans la hiérarchie du rite   |
| `--corpus`         | `./corpus`    | Racine du corpus YAML              |
| `--out`            | `./artifacts` | Répertoire de sortie               |
| `--i18n`           | _(désactivé)_ | Active la production des `.lits`   |
| `--include-drafts` | _(désactivé)_ | Compile les scopes marqués `DRAFT` |

---

## Nommage des artefacts

Le nom de fichier est le chemin de scope **aplati** avec `_` comme séparateur :

| Scope                    | Artefact                      |
| ------------------------ | ----------------------------- |
| `romanus/universale`     | `romanus_universale.kald`     |
| `romanus/nationalia/FR`  | `romanus_nationalia_FR.kald`  |
| `ambrosianus/universale` | `ambrosianus_universale.kald` |

Les `.lits` reçoivent le code langue en suffixe :
`romanus_universale_la.lits`, `romanus_universale_fr.lits`

---

## Structure du corpus i18n

Les dictionnaires i18n sont colocalisés avec le corpus, scope par scope :

```
corpus/romanus/
├── universale/
│   ├── i18n/
│   │   ├── la/
│   │   │   └── dominica_resurrectionis.yaml
│   │   └── fr/
│   │       └── dominica_ii_paschae.yaml
│   ├── sanctorale/
│   └── temporale/
└── nationalia/FR/
    ├── i18n/
    │   ├── la/
    │   │   └── jeanne_d_arc.yaml
    │   └── fr/
    │       └── jeanne_d_arc.yaml
    └── sanctorale/
```

La Forge traverse automatiquement toute la hiérarchie dans l'ordre :
`universale` → `continentalia/*` → `nationalia/*` → `dioecesana/*` → `ordines/*`

Les scopes plus spécifiques surchargent les scopes parents (last-write-wins).

### Format d'un fichier i18n

```yaml
version: 1
history:
  - from: 1969 # optionnel — défaut : 1969
    to: 2001 # optionnel — parsé pour lisibilité, non stocké
    label: "Dominica II Paschæ"
    annotation: "*In albis*" # optionnel, Markdown admis
  - from: 2002
    label: "Dominica II Paschæ"
    annotation: "Dominica in octava paschæ seu de sacra misericordia, *In albis*"
```

**Règles :**

- `label` est obligatoire dans chaque bloc.
- `from` est optionnel uniquement si la fête n'a qu'une seule entrée `history`.
- `annotation` est facultatif. Si absent, l'Engine reçoit `None` sans coût mémoire.
- Le formatage Markdown (`*italique*`) est transmis tel quel à l'UI cliente.
- Le latin (`la`) est la langue de fallback universelle — tout slug doit avoir un fichier latin.

---

## Scopes en cours de développement (DRAFT)

Tout scope contenant un fichier `DRAFT` à sa racine est ignoré par défaut :

```
corpus/romanus/dioecesana/VIENNE/
    DRAFT          ← ignoré sauf --include-drafts
    sanctorale/
```

---

## Stabilité des identifiants

Deux fichiers lock garantissent la stabilité des IDs entre compilations :

| Fichier                               | Rôle                             |
| ------------------------------------- | -------------------------------- |
| `corpus/{rite}/feast_registry.lock`   | FeastID stables par slug de fête |
| `corpus/{rite}/variant_registry.lock` | variant_id stables par scope     |

Ces fichiers sont **versionnés dans le repo**. Ne pas les supprimer ni les éditer manuellement.

---

## Vérification d'un artefact produit

Le Build ID affiché à la fin de la compilation (`build_id = 0x…`) correspond aux 8 premiers octets du SHA-256 du Data Body. Il est inscrit dans le header du `.kald` et dans chaque `.lits` compagnon — l'Engine rejette tout `.lits` dont le Build ID ne correspond pas au `.kald` chargé.

```
[kal-forge] ✓  ./artifacts/romanus_universale.kald
[kal-forge]    build_id = 0xf4ea5009b605cd1b
```
