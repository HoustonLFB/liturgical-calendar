# Décisions pour l'implémentation du module `LITS` (version 2)

## 1. Mutation du Layout Binaire (`.lits`)

L'invariant structurel de la **Entry Table** évolue pour supporter un second champ textuel sans indirection CPU supplémentaire. On abandonne la solution par "séparateur" au profit d'un **Double Offset** (DOD) pour garantir un accès $O(1)$ déterministe.

### Spécifications de l'Entrée (Entry Table)

Le passage de **10 octets à 14 octets** par entrée est acté.

| Champ                   | Type  | Taille | Rôle                                                            |
| :---------------------- | :---- | :----- | :-------------------------------------------------------------- |
| `feast_id`              | `u16` | 2B     | ID unique de la fête                                            |
| `from`                  | `u16` | 2B     | Année de début de validité                                      |
| `to`                    | `u16` | 2B     | Année de fin de validité                                        |
| **`label_offset`**      | `u32` | 4B     | Offset vers le nom principal (ex: "Dimanche de la Miséricorde") |
| **`annotation_offset`** | `u32` | 4B     | Offset vers la note liturgique (ex: "in albis")                 |

---

## 2. Nomenclature et Sémantique

Les termes retenus pour le schéma et le code sont :

1.  **`label`** : Le titre officiel/canonique de l'entrée.
2.  **`annotation`** : Précision, titre alternatif ou mention rubricale courte.

---

## 3. Logique de la Forge (AOT Pipeline)

La Forge doit assurer l'intégrité des données avant sérialisation :

- **Résolution des Fallbacks** : Si le champ `label` est manquant dans la juridiction cible (ex: `FR`), la Forge injecte le label de la source parente (`lat` par défaut).
- **Gestion du Vide** : Si aucune `annotation` n'est définie pour une entrée, `annotation_offset` doit être forcé à **`0`**. L'Engine interprétera un offset nul comme l'absence de donnée.
- **Déterminisme** : L'ordre d'écriture dans la table reste trié par `(feast_id ASC, from ASC)`.

---

## 4. Impact sur les Composants

### Forge (`lits_writer.rs`)

- Mettre à jour la signature de `write_lits` pour accepter une `LabelTable` étendue (struct/map incluant `label` et `annotation`).
- Calculer le nouveau `pool_offset` dans le header (32B) en fonction de `entry_count * 14`.

### Engine (`lits_provider.rs`)

- Adapter le lecteur pour "jumper" de 14 octets lors de la recherche binaire.
- Le `LitsProvider` doit exposer une méthode retournant un tuple ou une struct : `{ label: &str, annotation: Option<&str> }`.

---

**Statut :** Prêt pour implémentation. Zéro rétrocompatibilité requise (Phase 0.3).
