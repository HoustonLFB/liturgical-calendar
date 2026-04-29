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
