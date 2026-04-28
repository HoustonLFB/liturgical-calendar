# Spécifications d'Ingestion — Sources `i18n` (Pipeline `.lits`)

## 1. Architecture des Fichiers : "Flat DOD-Mirroring"

L'organisation des fichiers de traduction calque la hiérarchie de résolution des logiques calendaires (`.kald`), tout en conservant une structure à plat ("flat") pour minimiser la fragmentation.

- **Atomicité** : Un fichier YAML par identifiant liturgique (slug).
- **Colocation** : Le dossier `i18n/` est placé à la racine de la juridiction cible (`universale/`, `nationalia/FR/`, etc.).
- **Aplatissement** : À l'intérieur de `i18n/{locale}/`, tous les fichiers sont regroupés sans sous-dossiers chronologiques.

## 2. Schéma YAML et Versionnage Temporel

Afin d'assurer une cohérence structurelle avec les fichiers de logique (`.kald`), le schéma i18n adopte un bloc `history` permettant de gérer l'évolution des noms (ex: canonisations).

```yaml
version: 1
history:
  - from: 2011
    to: 2013
    label: "B. Ioannes Paulus II, pp."
  - from: 2014
    label: "S. Ioannes Paulus II, pp."
    annotation: "Memoriae dies"
```

**Contrat de parsing (`lits_writer.rs`) :**

- **`version`** : Obligatoire (u16).
- **`from` / `to`** : Optionnels. Valeurs par défaut : `1969` / `2399`.
- **`label`** : Obligatoire (String).
- **`annotation`** : Optionnel (String). Si absent, la Forge inscrit `0xFFFF_FFFF` dans l'offset binaire.

## 3. Formatage et Tokenization (Markdown)

- **Standard** : Markdown léger.
- **Mise en œuvre** : Utilisation des astérisques pour l'emphase (ex: `*in albis*`).
- **Philosophie** : Agnosticisme total de la Forge et de l'Engine. Le rendu (HTML, ANSI, ou Mobile) est délégué au client final.
- **Optimisation** : Gain de poids dans le _String Pool_ par rapport aux balises HTML.

## 4. Algorithme de Résolution et Fallback AOT

La Forge construit une `LabelTable` unique par `CompilationTarget`. Pour chaque `slug` présent dans le calendrier :

1.  **Shadowing (Vertical)** : La juridiction la plus spécifique (ex: `nationalia/FR`) écrase les définitions de `universale`.
2.  **Temporalité (Horizontal)** : La Forge sélectionne l'entrée dans `history` dont la plage `[from, to]` couvre l'année en cours de traitement.
3.  **Fallback Linguistique** : Si aucune traduction n'existe pour la locale demandée (ex: `fr-CA`), la Forge cherche en `fr`, puis en `la` (latin).
4.  **Autonomie** : Le binaire `.lits` résultant est complet ; l'Engine n'effectue aucune recherche de fallback à l'exécution.

## 5. Synthèse des Décisions Techniques

| Sujet                 | Décision                                                                                                   |
| :-------------------- | :--------------------------------------------------------------------------------------------------------- |
| **Extension**         | **`.lits`** (Liturgical Strings).                                                                          |
| **Structure Source**  | Atomique avec liste `history` (alignée sur `.kald`).                                                       |
| **Convention Locale** | IETF / BCP 47 (ex: `fr`, `fr-CA`).                                                                         |
| **Binaire**           | Entry Table de **14 octets** (`feast_id: u16`, `from: u16`, `to: u16`, `name_off: u32`, `annot_off: u32`). |
| **Null Marker**       | **`0xFFFF_FFFF`** pour les annotations absentes.                                                           |
| **Recherche Engine**  | Recherche binaire (O(log n)) sur table triée par `(feast_id, from)`.                                       |
