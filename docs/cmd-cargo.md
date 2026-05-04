# Commandes standard

```
cargo build -p liturgical-calendar-core
cargo test -p liturgical-calendar-core
cargo clippy -p liturgical-calendar-core -- -D warnings
cargo tree -p liturgical-calendar-core
```

```
cargo build -p liturgical-calendar-forge
cargo test -p liturgical-calendar-forge
cargo clippy -p liturgical-calendar-forge -- -D warnings
cargo tree -p liturgical-calendar-forge
```

Forger un binaire `.kald` :

```
cargo run -p liturgical-calendar-forge --bin kal-forge -- \
    --rite romanus \
    --scope universale \
    --corpus ./corpus \
    --out ./artifacts
```

Forger un binaire `.lits` :

```
cargo run -p liturgical-calendar-forge --bin kal-forge -- \
    --rite romanus \
    --scope universale \
    --corpus ./corpus \
    --out ./artifacts \
    --i18n
```

Voir les entrées du 20 janvier 2026 (doy 19) :

```
kal-read --kald ./artifacts/romanus_universale.kald --lits ./artifacts/romanus_universale_la.lits --year 2026 --doy 19
```

Voir toutes les fêtes du 1 au 31 janvier 2026 :

```
for doy in $(seq 0 30); do
    echo -n "$(printf '%3d' $doy)  "
    kal-read --kald ./artifacts/romanus_universale.kald \
             --lits ./artifacts/romanus_universale_la.lits \
             --year 2026 --doy $doy \
    | grep -E "label|feast_id|precedence|nature|\["
done
```
