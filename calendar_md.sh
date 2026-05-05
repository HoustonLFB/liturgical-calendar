#!/usr/bin/env bash

# Affiche en Markdown la liste des jours de l’année liturgique (calendrier DOD)
# avec DOY, date (JJ mmm) et les labels (principal | secondaires).
# Usage : ./calendar_md.sh [-d] [-f KALD_FILE] [-l LITS_FILE] ANNÉE
# Exemple : ./calendar_md.sh 2026 > calendar_2026.md
# Pour les dimanches uniquement :
# ./calendar_md.sh -d 2026 > domini_2026.md

set -euo pipefail

# --- Valeurs par défaut ---
KALD_FILE="./artifacts/romanus_universale.kald"
LITS_FILE="./artifacts/romanus_universale_la.lits"
SUNDAYS_ONLY=false

# --- Aide ---
usage() {
    cat <<EOF
Usage: $0 [-d] [-f KALD_FILE] [-l LITS_FILE] YEAR

Produit une liste Markdown de tous les jours de l’année liturgique
(ou seulement les dimanches avec -d) au format :
  - **DOY** (JJ mmm) Label1 | Label2

Options :
  -d    dimanches uniquement (commence au DOY 3)
  -f    chemin vers le fichier .kald (défaut : $KALD_FILE)
  -l    chemin vers le fichier .lits (défaut : $LITS_FILE)
  -h    cette aide

Exemples :
  $0 2026
  $0 -d 2026 > dimanches_2026.md
  $0 -f ./data/romain.kald -l ./data/romain.lits 1984 > calendar_1984.md
EOF
    exit 0
}

# --- Analyse des arguments ---
while getopts "df:l:h" opt; do
    case "$opt" in
        d) SUNDAYS_ONLY=true ;;
        f) KALD_FILE="$OPTARG" ;;
        l) LITS_FILE="$OPTARG" ;;
        h) usage ;;
        *) usage ;;
    esac
done
shift $((OPTIND-1))

if [ $# -ne 1 ]; then
    echo "Erreur : année requise."
    usage
fi

YEAR=$1

# --- Vérification des fichiers ---
if [ ! -f "$KALD_FILE" ]; then
    echo "Erreur : fichier KALD introuvable : $KALD_FILE"
    exit 1
fi
if [ ! -f "$LITS_FILE" ]; then
    echo "Erreur : fichier LITS introuvable : $LITS_FILE"
    exit 1
fi

# --- Année bissextile ? ---
if (( (YEAR % 4 == 0 && YEAR % 100 != 0) || YEAR % 400 == 0 )); then
    LEAP_YEAR=true
else
    LEAP_YEAR=false
fi

# --- Fonction d’affichage d’un jour ---
print_day() {
    local doy=$1

    # Décalage réel par rapport au 1er janvier (en années non bissextiles on saute le DOY 59)
    if $LEAP_YEAR; then
        offset=$doy
    else
        if [ "$doy" -lt 59 ]; then
            offset=$doy
        else
            offset=$((doy - 1))
        fi
    fi

    # Date au format "JJ mmm" en minuscules
    date_str=$(LC_TIME=C date -d "$YEAR-01-01 +$offset days" +"%d %b" | tr '[:upper:]' '[:lower:]')

    # Récupération des labels (principal et secondaires)
    labels=$(kal-read --kald "$KALD_FILE" --lits "$LITS_FILE" --year "$YEAR" --doy "$doy" |
             awk '
                 /^[[:space:]]*label[[:space:]]*:/ {
                     sub(/^[^:]*:[[:space:]]*/, "")
                     main = $0
                 }
                 # ne rien faire pour les lignes [Padding …], on laisse main vide
                 /^[[:space:]]+\[[0-9]+\][[:space:]]/ && !/feast_id/ {
                     sub(/^[[:space:]]+\[[0-9]+\][[:space:]]*/, "")
                     if ($0 != "") secondary[++s] = $0
                 }
                 END {
                     sep = ""
                     if (main != "") {
                         printf "%s", main
                         sep = " | "
                     }
                     for (i=1; i<=s; i++) {
                         printf "%s%s", sep, secondary[i]
                         sep = " | "
                     }
                 }
             ')

    # Sortie en liste Markdown
    if [ -n "$labels" ]; then
        printf -- '- **%03d** (%s) %s\n' "$doy" "$date_str" "$labels"
    else
        printf -- '- **%03d** (%s)\n' "$doy" "$date_str"
    fi
}

# --- Boucle principale ---
if $SUNDAYS_ONLY; then
    doy=3
    while [ $doy -le 365 ]; do
        if ! $LEAP_YEAR && [ "$doy" -eq 59 ]; then
            doy=60
            continue
        fi
        print_day "$doy"
        doy=$((doy + 7))
        if ! $LEAP_YEAR && [ "$doy" -eq 59 ]; then
            doy=60
        fi
    done
else
    for doy in $(seq 0 365); do
        if ! $LEAP_YEAR && [ "$doy" -eq 59 ]; then
            continue
        fi
        print_day "$doy"
    done
fi
