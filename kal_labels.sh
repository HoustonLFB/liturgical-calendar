#!/usr/bin/env bash

# Affiche pour chaque jour de l'année (ou seulement les dimanches)
# son DOY, sa date (JJ mmm) et tous les labels liturgiques (principal | secondaires)
# en tenant compte de l'architecture DOD (index 59 = 29 février).

# Usage : ./kal_labels.sh [-d] [-f FICHIER_KALD] [-l FICHIER_LITS] ANNÉE
## Exemple : ./kal_labels.sh 2026 > calendrier_2026.txt

set -euo pipefail

# --- Valeurs par défaut ---
KALD="./artifacts/romanus_universale.kald"
LITS="./artifacts/romanus_universale_la.lits"
DIMANCHES_ONLY=false

# --- Fonction d'aide ---
usage() {
    cat <<EOF
Usage: $0 [-d] [-f KALD] [-l LITS] ANNÉE

Affiche le DOY, la date (jour mois) et les labels de chaque jour de l'année liturgique
(calendrier DOD). Les labels secondaires sont concaténés avec " | ".

Options :
  -d    N'afficher que les dimanches (commence au DOY 3, premier dimanche de l'année)
  -f    Chemin vers le fichier .kald (défaut : $KALD)
  -l    Chemin vers le fichier .lits (défaut : $LITS)
  -h    Affiche cette aide

Exemples :
  $0 2026
  $0 -d 2026
  $0 -f ./data/romain.kald -l ./data/romain.lits 1984
EOF
    exit 0
}

# --- Analyse des arguments ---
while getopts "df:l:h" opt; do
    case "$opt" in
        d) DIMANCHES_ONLY=true ;;
        f) KALD="$OPTARG" ;;
        l) LITS="$OPTARG" ;;
        h) usage ;;
        *) usage ;;
    esac
done
shift $((OPTIND-1))

if [ $# -ne 1 ]; then
    echo "Erreur : vous devez fournir l'année."
    usage
fi

YEAR=$1

# --- Vérification des fichiers ---
if [ ! -f "$KALD" ]; then
    echo "Erreur : fichier KALD introuvable : $KALD"
    exit 1
fi
if [ ! -f "$LITS" ]; then
    echo "Erreur : fichier LITS introuvable : $LITS"
    exit 1
fi

# --- Bissextile ? ---
if (( (YEAR % 4 == 0 && YEAR % 100 != 0) || YEAR % 400 == 0 )); then
    BISSEXTILE=true
else
    BISSEXTILE=false
fi

# --- Fonction d'affichage d'un jour ---
afficher_jour() {
    local doy=$1

    # Calcul de l'offset réel en jours depuis le 01 janvier
    if $BISSEXTILE; then
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

    # Extraction des labels
    labels=$(kal-read --kald "$KALD" --lits "$LITS" --year "$YEAR" --doy "$doy" |
             awk '
                 /^[[:space:]]*label[[:space:]]*:/ {
                     sub(/^[^:]*:[[:space:]]*/, "")
                     main = $0
                 }
                 /^\[Padding/ { main = $0 }
                 /^[[:space:]]+\[[0-9]+\][[:space:]]/ && !/feast_id/ {
                     sub(/^[[:space:]]+\[[0-9]+\][[:space:]]*/, "")
                     if ($0 != "") secondary[++s] = $0
                 }
                 END {
                     if (main == "") main = "[Pas de célébration]"
                     printf "%s", main
                     for (i=1; i<=s; i++) printf " | %s", secondary[i]
                 }
             ')

    printf '%3d  %s  %s\n' "$doy" "$date_str" "$labels"
}

# --- Boucle sur les jours ---
if $DIMANCHES_ONLY; then
    # Premier dimanche : DOY 3 (4 janvier en 2026, mais on garde 3 car DOD invariant)
    doy=3
    while [ $doy -le 365 ]; do
        # Saut conditionnel du DOY 59
        if ! $BISSEXTILE && [ "$doy" -eq 59 ]; then
            doy=60
            continue
        fi
        afficher_jour "$doy"
        doy=$((doy + 7))
        # Éviter de se poser sur 59 après incrément
        if ! $BISSEXTILE && [ "$doy" -eq 59 ]; then
            doy=60
        fi
    done
else
    for doy in $(seq 0 365); do
        if ! $BISSEXTILE && [ "$doy" -eq 59 ]; then
            continue
        fi
        afficher_jour "$doy"
    done
fi
