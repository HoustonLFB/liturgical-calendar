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

Tout builder :

```
cargo build --workspace
```

Tout tester :

```
cargo test --workspace
```

Lancer clippmy sur le projet :

```
cargo clippy --workspace
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

---

Lister les entrées du 20 janvier 2026 (doy 19) :

```
kal-read --kald ./artifacts/romanus_universale.kald --lits ./artifacts/romanus_universale_la.lits --year 2026 --doy 19
```

Lister tous les jours de l'année 2026, uniquement doy + date + label :

```
for doy in $(seq 0 365); do
    [ "$doy" -eq 59 ] && continue   # saut du 29 février DOD

    # Calcul du décalage réel (le 29 février n’existe pas en 2026)
    if [ "$doy" -lt 59 ]; then
        offset=$doy
    else
        offset=$((doy - 1))
    fi

    # Date au format "JJ mmm" en minuscules
    date_str=$(LC_TIME=C date -d "2026-01-01 +$offset days" +"%d %b" | tr '[:upper:]' '[:lower:]')

    # Extraction du label principal et des labels secondaires
    labels=$(kal-read --kald ./artifacts/romanus_universale.kald \
                      --lits ./artifacts/romanus_universale_la.lits \
                      --year 2026 --doy $doy |
             awk '
                 # Ligne du label principal
                 /^[[:space:]]*label[[:space:]]*:/ {
                     sub(/^[^:]*:[[:space:]]*/, "")
                     main = $0
                 }
                 # Jour sans célébration : ligne [Padding Entry …]
                 /^\[Padding/ {
                     main = $0
                 }
                 # Célébrations secondaires : [N] Texte
                 /^[[:space:]]+\[[0-9]+\][[:space:]]/ && !/feast_id/ {
                     sub(/^[[:space:]]+\[[0-9]+\][[:space:]]*/, "")
                     if ($0 != "") secondary[++s] = $0
                 }
                 END {
                     printf "%s", main
                     for (i=1; i<=s; i++) printf " | %s", secondary[i]
                 }
             ')

    printf '%3d  %s  %s\n' "$doy" "$date_str" "$labels"
done
```

Lister toutes les fêtes du 1 au 31 janvier 2026, avec toutes leur infos :

```
for doy in $(seq 0 30); do
    echo -n "$(printf '%3d' $doy)  "
    kal-read --kald ./artifacts/romanus_universale.kald \
             --lits ./artifacts/romanus_universale_la.lits \
             --year 2026 --doy $doy \
    | grep -E "label|feast_id|precedence|nature|\["
done
```

Lister tous les dimanches de l'année 2026 (commence à doy 3), uniquement doy + label :

```
doy=3
while [ $doy -le 365 ]; do
    if [ $doy -ne 59 ]; then
        label=$(kal-read --kald ./artifacts/romanus_universale.kald \
                         --lits ./artifacts/romanus_universale_la.lits \
                         --year 2026 --doy $doy |
                grep -v '^\s*\[' | grep "label" | sed 's/^.*label\s*:\s*//')
        printf '%3d  %s\n' $doy "$label"
    fi
    doy=$(( doy + 7 ))
    [ $doy -eq 59 ] && doy=60
done
```
