import os
import re
from pathlib import Path

def refine_labels(corpus_dir: str):
    corpus_path = Path(corpus_dir)
    modified_count = 0

    # On ne cible QUE les fichiers dans les dossiers i18n
    for filepath in corpus_path.rglob("i18n/**/*.yaml"):
        try:
            with open(filepath, "r", encoding="utf-8") as f:
                lines = f.readlines()

            new_lines = []
            changed = False
            
            for line in lines:
                if line.strip().startswith("label:"):
                    original = line
                    
                    # 1. Particules latines et conjonctions (scopées par des espaces)
                    line = line.replace(" Et ", " et ")
                    line = line.replace(" De ", " de ")
                    line = line.replace(" In ", " in ")
                    line = line.replace(" A ", " a ") # Ex: Teresiae a Puero Iesu
                    
                    # 2. Traitement sécurisé des chiffres romains via Regex
                    # Note : .title() a déjà mis la 1ère lettre en majuscule et le reste en minuscule.
                    # On cherche ces mots exacts et on les passe entièrement en majuscules.
                    # Le \b protège le "ii" à la fin de "Ordinarii".
                    roman_pattern = r'\b(Ii|Iii|Iv|Vi|Vii|Viii|Ix|Xi|Xii|Xiii|Xiv|Xv|Xvi|Xvii|Xviii|Xix|Xx|Xxi|Xxii|Xxiii|Xxiv|Xxv|Xxvi|Xxvii|Xxviii|Xxix|Xxx|Xxxi|Xxxii|Xxxiii|Xxxiv)\b'
                    
                    # La fonction lambda prend le mot matché et le met en majuscule
                    line = re.sub(roman_pattern, lambda m: m.group(0).upper(), line)

                    # 3. Remplacement des ligatures (à faire en dernier)
                    line = line.replace("ae", "æ")
                    line = line.replace("Ae", "Æ")
                    
                    if line != original:
                        changed = True
                    
                new_lines.append(line)

            if changed:
                with open(filepath, "w", encoding="utf-8") as f:
                    f.writelines(new_lines)
                modified_count += 1
                print(f"Raffiné : {filepath.relative_to(corpus_path)}")

        except Exception as e:
            print(f"Erreur sur {filepath}: {e}")

    print(f"\nTerminé. {modified_count} fichiers i18n mis à jour.")

if __name__ == "__main__":
    refine_labels("./corpus")
