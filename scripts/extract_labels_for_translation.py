#!/usr/bin/env python3
"""
Script 1 : Extrait tous les labels latins et crée un fichier de correspondance.
Usage : python3 extract_labels_for_translation.py dossier_la/ fichier_sortie.txt
"""

import os
import sys
import yaml

def extract_labels(root_dir):
    """Extrait tous les labels avec leur contexte pour traduction."""
    translations = []
    
    for root, dirs, files in os.walk(root_dir):
        for file in sorted(files):
            if file.endswith(('.yaml', '.yml')):
                filepath = os.path.join(root, file)
                rel_path = os.path.relpath(filepath, root_dir)
                
                try:
                    with open(filepath, 'r', encoding='utf-8') as f:
                        data = yaml.safe_load(f)
                    
                    if data and 'history' in data:
                        for i, entry in enumerate(data['history']):
                            if 'label' in entry:
                                translations.append({
                                    'file': rel_path,
                                    'index': i,
                                    'from': entry.get('from', ''),
                                    'label_la': entry['label'],
                                    'label_fr': ''  # À remplir
                                })
                except Exception as e:
                    print(f"⚠️  Erreur {filepath}: {e}", file=sys.stderr)
    
    return translations

def save_translation_file(translations, output_file):
    """Sauvegarde le fichier de correspondance à traduire."""
    with open(output_file, 'w', encoding='utf-8') as f:
        f.write("# FICHIER DE TRADUCTION - Calendrier liturgique\n")
        f.write("# Traduisez chaque 'label_fr' en conservant exactement la structure\n")
        f.write(f"# Total : {len(translations)} labels à traduire\n")
        f.write("# Format : FICHIER | INDEX | ANNÉE | LABEL_LATIN | LABEL_FRANÇAIS\n")
        f.write("=" * 100 + "\n\n")
        
        for item in translations:
            f.write(f"FICHIER: {item['file']}\n")
            f.write(f"INDEX: {item['index']}\n")
            if item['from']:
                f.write(f"ANNÉE: {item['from']}\n")
            f.write(f"LABEL_LA: {item['label_la']}\n")
            f.write(f"LABEL_FR: \n")  # Ligne vide à remplir
            f.write("-" * 60 + "\n\n")

def main():
    if len(sys.argv) != 3:
        print("Usage: python3 extract_labels_for_translation.py <dossier_la> <fichier_sortie.txt>")
        sys.exit(1)
    
    la_dir = sys.argv[1]
    output_file = sys.argv[2]
    
    translations = extract_labels(la_dir)
    
    if not translations:
        print("❌ Aucun label trouvé !")
        sys.exit(1)
    
    save_translation_file(translations, output_file)
    
    print(f"✅ {len(translations)} labels extraits dans '{output_file}'")
    print(f"📝 Ouvrez ce fichier, traduisez les LABEL_FR, puis lancez le script d'application.")

if __name__ == "__main__":
    main()
