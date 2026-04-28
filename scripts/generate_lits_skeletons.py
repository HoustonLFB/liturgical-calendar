import os
from pathlib import Path

def generate_i18n_skeletons(corpus_dir: str, lang: str = "la"):
    corpus_path = Path(corpus_dir)
    created_count = 0

    # Itération sur tous les fichiers YAML existants (la logique .kald)
    for filepath in corpus_path.rglob("*.yaml"):
        # Éviter de traiter les fichiers i18n s'ils existent déjà
        if "i18n" in filepath.parts:
            continue

        # 1. Détection de la racine de la juridiction
        # On cherche l'index de 'sanctorale' ou 'temporale' dans le chemin
        try:
            if "sanctorale" in filepath.parts:
                anchor_idx = filepath.parts.index("sanctorale")
            elif "temporale" in filepath.parts:
                anchor_idx = filepath.parts.index("temporale")
            else:
                continue # Fichier hors structure (ex: config globale)

            # La racine est tout ce qui précède l'ancrage
            juridiction_path = Path(*filepath.parts[:anchor_idx])

            # 2. Extraction et transformation des chaînes
            filename = filepath.name
            slug = filepath.stem # Nom sans extension (.yaml)
            
            # Transformation : underscores -> espaces, puis Title Case
            label = slug.replace('_', ' ').title()

            # 3. Construction du chemin cible ("Flat structure")
            target_dir = juridiction_path / "i18n" / lang
            target_dir.mkdir(parents=True, exist_ok=True)
            target_file = target_dir / filename

            # 4. Écriture atomique (Safe : ne pas écraser l'existant)
            if not target_file.exists():
                yaml_content = (
                    f"version: 1\n"
                    f"history:\n"
                    f"  - from: 1969\n"
                    f"    label: \"{label}\"\n"
                )
                with open(target_file, "w", encoding="utf-8") as f:
                    f.write(yaml_content)
                
                created_count += 1
                print(f"Généré : {target_file.relative_to(corpus_path.parent)}")

        except Exception as e:
            print(f"Erreur sur {filepath}: {e}")

    print(f"\nOpération terminée. {created_count} fichiers '{lang}' générés.")

if __name__ == "__main__":
    # Ajustez le chemin relatif si le script est lancé depuis un autre répertoire
    TARGET_CORPUS = "./corpus" 
    
    if Path(TARGET_CORPUS).exists():
        generate_i18n_skeletons(TARGET_CORPUS, "la")
    else:
        print(f"Erreur : Le dossier {TARGET_CORPUS} est introuvable.")
