# liturgical-calendar

Façade du calendrier liturgique catholique (Novus Ordo, 1969–2399).

Point d'entrée unique pour les intégrateurs Rust. Ré-exporte l'intégralité de la surface publique de [`liturgical-calendar-core`](https://crates.io/crates/liturgical-calendar-core) — moteur `no_std`, `no_alloc`, accès O(1) par `(année, jour_de_l_année)`.

## Utilisation

```toml
[dependencies]
liturgical-calendar = "0.1"
```

```rust
use liturgical_calendar::{kal_read_entry, CalendarEntry, KAL_ENGINE_OK};

let kald: Vec<u8> = std::fs::read("romanus_universale.kald")?;
let mut entry = CalendarEntry::zeroed();

let rc = unsafe { kal_read_entry(kald.as_ptr(), kald.len(), 2025, 109, &mut entry) };
assert_eq!(rc, KAL_ENGINE_OK);

if !entry.is_padding() {
    let nature     = entry.nature()?;
    let precedence = entry.precedence()?;
}
```

```rust
// Labels i18n (zero-copy)
use liturgical_calendar::lits_provider::LitsProvider;

let lits: Vec<u8> = std::fs::read("romanus_universale_la.lits")?;
let provider = LitsProvider::new(&lits)?;

if let Some(entry) = provider.get(feast_id, 2025) {
    println!("{}", entry.label);
    if let Some(ann) = entry.annotation { println!("{}", ann); }
}
```

## Intégration C / FFI

La génération du header C (`liturgical_calendar.h`) s'effectue directement depuis [`liturgical-calendar-core`](https://crates.io/crates/liturgical-calendar-core), là où résident les symboles `#[no_mangle] extern "C"` :

```bash
cargo build -p liturgical-calendar-core --features gen-headers
```

Les intégrateurs FFI ciblent `liturgical-calendar-core` pour compiler la bibliothèque (`.a` / `.so`) et récupérer l'interface C-ABI. La façade n'intervient pas dans ce flux.

```c
#include "liturgical_calendar.h"

CalendarEntry entry;
int rc = kal_read_entry(kald_data, kald_len, 2025, 109, &entry);
```

## Architecture

```
YAML corpus
  → kal-forge  (build-time, liturgical-calendar-forge)
    → romanus_universale.kald
    → romanus_universale_la.lits
      → liturgical_calendar::kal_read_entry   (runtime, O(1))
      → liturgical_calendar::lits_provider    (zero-copy)
```

Les artefacts `.kald` et `.lits` sont produits par [`liturgical-calendar-forge`](https://crates.io/crates/liturgical-calendar-forge). Ce crate ne contient aucune règle liturgique — il consomme uniquement les artefacts pré-compilés.

## Crates du Projet

| Crate | Rôle |
|---|---|
| `liturgical-calendar` | Façade — point d'entrée utilisateur (ce crate) |
| [`liturgical-calendar-core`](https://crates.io/crates/liturgical-calendar-core) | Engine `no_std` — lecture des artefacts binaires |
| [`liturgical-calendar-forge`](https://crates.io/crates/liturgical-calendar-forge) | Compilateur YAML → `.kald` + `.lits` |
