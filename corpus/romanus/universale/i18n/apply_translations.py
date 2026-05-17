#!/usr/bin/env python3
"""
Script 2 corrigé - Parse correctement le fichier de traduction.
"""
import os
import sys
import re
from collections import defaultdict

def parse_translation_file(filepath):
    """Parse le fichier de correspondance traduit."""
    translations = defaultdict(dict)
    
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # Séparer les blocs
    blocks = content.split('-' * 60)
    
    processed = 0
    for block in blocks:
        block = block.strip()
        
        # Ignorer les blocs vides ou qui ne contiennent pas FICHIER:
        if not block or 'FICHIER:' not in block:
            continue
        
        current = {}
        for line in block.split('\n'):
            line = line.strip()
            if line.startswith('FICHIER:'):
                current['file'] = line.replace('FICHIER:', '').strip()
            elif line.startswith('INDEX:'):
                try:
                    current['index'] = int(line.replace('INDEX:', '').strip())
                except ValueError:
                    print(f"⚠️  INDEX invalide dans bloc: {line}")
                    continue
            elif line.startswith('LABEL_FR:'):
                # Prendre tout après LABEL_FR: (même vide)
                label_fr = line[len('LABEL_FR:'):].strip()
                current['label_fr'] = label_fr
        
        if 'file' in current and 'index' in current:
            # Toujours ajouter, même si label_fr est vide
            translations[current['file']][current['index']] = current.get('label_fr', '')
            processed += 1
            
            # Afficher les 5 premiers pour vérification
            if processed <= 5:
                label_preview = current.get('label_fr', '')[:50]
                print(f"✅ {current['file']}[{current['index']}] = '{label_preview}'")
    
    print(f"📊 Total labels parsés : {processed}")
    
    # Vérifier combien ont une traduction non vide
    non_empty = sum(1 for file_labels in translations.values() 
                    for label in file_labels.values() if label)
    print(f"📊 Labels avec traduction : {non_empty}")
    print(f"📊 Labels vides (à traduire) : {processed - non_empty}")
    
    return translations

def replace_labels_in_file(filepath, translations_dict):
    """
    Remplace uniquement les labels dans le fichier YAML.
    Préserve tous les commentaires et la structure.
    """
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
    
    modified = False
    lines = content.split('\n')
    current_history_index = -1
    in_history = False
    
    for i, line in enumerate(lines):
        stripped = line.strip()
        
        # Détecter l'entrée dans history
        if stripped == 'history:':
            in_history = True
            continue
        
        # Détecter un nouvel élément avec "- from:"
        if in_history and re.match(r'\s*- from:', line):
            current_history_index += 1
        
        # Remplacer le label si on a une traduction pour cet index
        if in_history and current_history_index in translations_dict:
            if re.match(r'\s*label:', line):
                old_label = line
                new_label_value = translations_dict[current_history_index]
                
                # Préserver l'indentation
                indent = line[:len(line) - len(line.lstrip())]
                
                if new_label_value:
                    # Échapper les guillemets doubles dans la traduction
                    escaped_label = new_label_value.replace('"', '\\"')
                    new_line = f'{indent}label: "{escaped_label}"'
                else:
                    # Label vide pour les traductions non faites
                    new_line = f'{indent}label: ""'
                
                if new_line != old_label:
                    lines[i] = new_line
                    modified = True
                    if new_label_value:
                        print(f"  ✅ Index {current_history_index}: traduit")
                    else:
                        print(f"  ⚠️  Index {current_history_index}: laissé vide (traduction manquante)")
    
    if modified:
        with open(filepath, 'w', encoding='utf-8') as f:
            f.write('\n'.join(lines))
        return True
    
    return False

def main():
    if len(sys.argv) != 3:
        print("Usage: python3 apply_translations.py <fichier_traductions.txt> <dossier_fr_fr>")
        sys.exit(1)
    
    translations_file = sys.argv[1]
    fr_dir = sys.argv[2]
    
    print("📖 Lecture du fichier de traductions...")
    translations = parse_translation_file(translations_file)
    
    total_files = len(translations)
    total_labels = sum(len(indices) for indices in translations.values())
    print(f"\n🔍 {total_files} fichiers à traiter, {total_labels} labels à modifier\n")
    
    if total_labels == 0:
        print("❌ Aucune traduction trouvée dans le fichier !")
        sys.exit(1)
    
    updated_files = 0
    errors = []
    empty_count = 0
    
    for rel_path, indices in translations.items():
        filepath = os.path.join(fr_dir, rel_path)
        
        if not os.path.exists(filepath):
            errors.append(f"❌ Fichier non trouvé : {rel_path}")
            continue
        
        try:
            print(f"📝 Traitement de {rel_path}...")
            
            # Compter les labels vides
            empty_in_file = sum(1 for label in indices.values() if not label)
            empty_count += empty_in_file
            
            if replace_labels_in_file(filepath, indices):
                updated_files += 1
                print(f"  ✅ Fichier modifié avec succès\n")
            else:
                print(f"  ℹ️  Aucune modification nécessaire\n")
                
        except Exception as e:
            errors.append(f"⚠️  Erreur {rel_path}: {e}")
    
    print("=" * 60)
    print(f"✅ {updated_files} fichiers modifiés avec succès")
    print(f"📊 {total_labels} labels traités au total")
    if empty_count > 0:
        print(f"⚠️  {empty_count} labels sont restés vides (traduction manquante)")
        print("💡 Pour les trouver : grep -r 'label: \"\"' fr_FR/")
    
    if errors:
        print(f"\n⚠️  {len(errors)} erreurs :")
        for error in errors:
            print(f"  {error}")

if __name__ == "__main__":
    main()
