# liturgical-calendar-core

Engine de lecture du calendrier liturgique catholique (Novus Ordo, 1969–2399).

Accès `O(1)` à n'importe quel jour de n'importe quelle année par simple offset arithmétique dans un buffer pré-compilé. Aucun calcul liturgique à l'exécution — toute l'intelligence est déportée dans la Forge ([`liturgical-calendar-forge`](https://crates.io/crates/liturgical-calendar-forge)).

## Caractéristiques

- `no_std`, `no_alloc` — embarquable partout
- Interface C-ABI (`extern "C"`) — iOS, Android, WebAssembly, systèmes embarqués
- Zéro dépendance runtime
- Accès O(1) par `(année, jour_de_l_année)` via offset fixe
- SHA-256 + discriminant de layout dans le header — intégrité vérifiable au chargement

## Format binaire `.kald`

Un fichier `.kald` contient 431 années (1969–2399) × 366 slots × 8 octets :

```
Header     64 octets   magic, version, SHA-256, layout_discriminant
Data Body  1 261 968 octets  CalendarEntry × 157 746
Secondary Pool  variable    listes de fêtes secondaires par slot
```

`CalendarEntry` — stride constant 8 octets, little-endian :

```
primary_id      u16   FeastID (0 = slot vide / Padding Entry)
secondary_index u16   index dans le Secondary Pool
flags           u16   precedence[3:0] | color[7:4] | period[10:8] | nature[13:11]
secondary_count u8    nombre de fêtes secondaires
_reserved       u8    0x00
```

## Format binaire `.lits`

Fichier compagnon du `.kald`, un par langue (BCP 47). Fournit les labels et annotations associés à chaque fête pour une plage d'années :

```
Header       32 octets   magic, version, lang (6B), kald_build_id (8B), entry_count, pool_offset, pool_size
Entry Table  entry_count × 14 octets   (feast_id, from, to, label_offset, annotation_offset)
String Pool  variable    UTF-8 null-terminé
```

L'Engine rejette tout `.lits` dont le `kald_build_id` ne correspond pas au `.kald` chargé.

## API

```rust
use liturgical_calendar_core::{kal_read_entry, kal_validate_header, CalendarEntry, KAL_ENGINE_OK};

// Validation du fichier .kald au chargement
let rc = unsafe { kal_validate_header(kald.as_ptr(), kald.len(), std::ptr::null_mut()) };
assert_eq!(rc, KAL_ENGINE_OK);

// Lecture d'une entrée — O(1)
let mut entry = CalendarEntry::zeroed();
let rc = unsafe { kal_read_entry(kald.as_ptr(), kald.len(), 2025, 109, &mut entry) };
assert_eq!(rc, KAL_ENGINE_OK);

if !entry.is_padding() {
    let precedence = entry.precedence()?;
    let nature     = entry.nature()?;
    let color      = entry.color()?;
}
```

```rust
// Labels via LitsProvider (zero-copy)
use liturgical_calendar_core::lits_provider::LitsProvider;

let provider = LitsProvider::new(&lits_bytes)?;
if let Some(entry) = provider.get(feast_id, 2025) {
    println!("{}", entry.label);
    if let Some(ann) = entry.annotation { println!("{}", ann); }
}
```

## Codes de retour FFI

| Constante             | Valeur | Signification                                      |
|-----------------------|--------|----------------------------------------------------|
| `KAL_ENGINE_OK`       | 0      | Succès                                             |
| `KAL_ERR_NULL_PTR`    | -1     | Pointeur null                                      |
| `KAL_ERR_BUF_TOO_SMALL` | -2  | Buffer insuffisant                                 |
| `KAL_ERR_MAGIC`       | -3     | Magic bytes invalides                              |
| `KAL_ERR_VERSION`     | -4     | Version de format non supportée                   |
| `KAL_ERR_CHECKSUM`    | -5     | SHA-256 incorrect — fichier corrompu               |
| `KAL_ERR_FILE_SIZE`   | -6     | Taille incohérente avec le header                  |
| `KAL_ERR_INDEX_OOB`   | -7     | Année ou DOY hors plage                            |
| `KAL_ERR_POOL_OOB`    | -8     | Index Secondary Pool hors limites                  |
| `KAL_ERR_RESERVED`    | -9     | Champ réservé non nul                              |
| `KAL_ERR_SCHEMA`      | -10    | Discriminant de layout incompatible                |

## Intégration C / FFI

```c
#include "liturgical_calendar_core.h"

CalendarEntry entry;
int rc = kal_read_entry(kald_data, kald_len, 2025, 109, &entry);
if (rc == KAL_ENGINE_OK && entry.primary_id != 0) {
    uint8_t precedence = entry.flags & 0x000F;
}
```

Le header C est généré par `cbindgen` via le feature `gen-headers`.

## Génération des fichiers `.kald` et `.lits`

Les artefacts consommés par cet Engine sont produits par [`liturgical-calendar-forge`](https://crates.io/crates/liturgical-calendar-forge).
