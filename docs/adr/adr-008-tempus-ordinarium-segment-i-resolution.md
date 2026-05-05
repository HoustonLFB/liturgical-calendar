# ADR-008 — Résolution des dimanches du Temps Ordinaire (Segment I) : double ancrage post_epiphaniam / ante_adventum

**Statut** : Accepté  
**Date** : 2026-05-04  
**Contexte** : `liturgical-calendar-forge` — Étape 2 (Canonicalization)

---

## Contexte

Le Temps Ordinaire est fragmenté en deux segments annuels discontinus, séparés par le Temps de Carême, le Triduum pascal et le Temps pascal :

- **Segment I** : du lundi suivant le Baptême du Seigneur au mardi précédant le Mercredi des Cendres.
- **Segment II** : du lundi de Pentecôte au samedi précédant le premier dimanche de l'Avent.

L'implémentation initiale calculait tous les dimanches du Temps Ordinaire par une formule unique à rebours depuis l'Avent :

```rust
pub fn resolve_tempus_ordinarium(adventus_doy: u16, ordinal: u8) -> u16 {
    adventus_doy.saturating_sub(7 * (35 - ordinal as u16))
}
```

Cette formule est correcte pour le Segment II. Pour le Segment I, elle produit des DOY postérieurs à Pâques pour les ordinaux II–VIII — ces entrées atterrissaient hors Temps Ordinaire ou en collision avec le Temps pascal et disparaissaient du `.kald`.

Exemple 2026 (Avent = DOY 332, Pâques = DOY 95) :

```
Dominica II per annum : 332 − 7×33 = 101 > 95 (Pâques)  ✗
```

---

## Décision

**Deux formules de résolution distinctes sont introduites, sélectionnées par une fonction de dispatch.**

### Ancre Segment I

```rust
/// Premier lundi après le Baptême du Seigneur (= epiphania + 1).
pub fn resolve_tempus_ordinarium_post_epiphaniam(year: u16) -> u16 {
    resolve_epiphania(year) + 1
}
```

Cette ancre est insérée dans l'`AnchorTable` sous la clé `"tempus_ordinarium_post_epiphaniam"`.

### Formule Segment I

```
DOY(Dominica N) = (post_epiphaniam − 1) + 7·(N−1)
               = epiphania + 7·(N−1)
```

### Fonction de dispatch

```rust
pub fn resolve_tempus_ordinarium_dispatch(
    year:            u16,
    post_epiphaniam: u16,
    adventus_doy:    u16,
    ordinal:         u8,
) -> u16
```

Le seuil de basculement (Mercredi des Cendres = `compute_easter(year) − 46`) est calculé en interne. Si `seg1 < ash_wednesday` → Segment I ; sinon → Segment II.

La fonction retourne toujours un `u16` — aucun `Option`, aucune suppression conditionnelle (conformément à ADR-001).

### Correction pseudo-DOY (années non-bissextiles)

Le slot DOY 59 (29 février) n'existe pas en année non-bissextile. Une séquence de dimanches traversant ce slot sans y atterrir doit être décalée d'un jour : tout `seg1_raw ≥ 59` devient `seg1_raw + 1` en année non-bissextile. Cette correction est interne à `dispatch`.

---

## Conformité ADR-001 — INV-FORGE-4

INV-FORGE-4 interdit à toute fonction de résolution de consulter les `SeasonBoundaries` ou les slots résolus d'autres ancres.

`resolve_tempus_ordinarium_dispatch` calcule `ash_wednesday` directement depuis `compute_easter(year)` — aucune consultation de l'`AnchorTable` ni des `SeasonBoundaries`. La fonction est pure : `(year, params) → u16`, testable exhaustivement sur [1969, 2399].

`post_epiphaniam` et `adventus_doy` sont des paramètres d'entrée de formule, transmis explicitement par le callsite — ils ne sont pas "consultés" au sens d'INV-FORGE-4.

---

## Conformité ADR-002 — INV-FORGE-5

Aucun mécanisme `transfers` n'est utilisé. La correction de position (Segment I vs Segment II) est réalisée entièrement à l'Étape 2.

---

## Conformité ADR-003 — V4a

Le champ `ordinal` dans le bloc `mobile` YAML reste inchangé. `RegistryTemporality::Ordinal { ordinal }` est le seul point d'entrée ; `offset` n'est pas réutilisé.

---

## Justification

### 1. Deux segments → deux ancres, pas une formule unifiée

La formule à rebours `adventus − 7·(35−N)` encode l'invariant du Segment II : la distance à rebours depuis l'Avent est fixe. Le Segment I n'a pas cet invariant — il progresse à partir d'une ancre forward (le Baptême). Unifier les deux formules revient à forcer un invariant inexistant.

### 2. `ash_wednesday` interne vs. paramètre

Passer `ash_wednesday` depuis l'`AnchorTable` aurait violé INV-FORGE-4 ("n'inspecte pas les slots résolus d'autres ancres"). Le calcul interne `compute_easter(year) − 46` préserve la pureté de la fonction et évite un couplage implicite sur l'ordre d'insertion dans l'`AnchorTable`.

### 3. Correction pseudo-DOY : arithmétique, pas logique saisonnière

Le décalage `+1` sur les années non-bissextiles est une correction de l'arithmétique des DOY, pas une décision liturgique. Elle appartient à la couche de résolution d'ancre (Étape 2), pas à la Conflict Resolution (Étape 3).

---

## Conséquences

**Positives :**

- Les dimanches du Temps Ordinaire Segment I (ordinaux II–VIII) apparaissent correctement dans le `.kald` pour toutes les années [1969, 2399].
- `resolve_tempus_ordinarium_dispatch` : pure, testable exhaustivement, conforme INV-FORGE-4.
- L'`AnchorTable` gagne une entrée (`tempus_ordinarium_post_epiphaniam`), sans rupture de contrat avec les consumers existants.
- Aucune modification du corpus YAML ni de `parsing.rs`.

**Contraintes acceptées :**

- Le callsite `feast_doy` dans `resolution.rs` passe désormais `year` à `dispatch`. La signature de `feast_doy` passe de `(..., anchors)` à `(..., anchors, year)`.
- L'Étape 3 continue de recevoir des slots "fantômes" pour les ordinaux dont `seg1 ≥ ash_wednesday` (ex. Dominica VII en année à Pâques précoce) via la formule Segment II — elle les élimine par Conflict Resolution, conformément à ADR-001 §Conséquences.

---

## Règle dérivée

> **INV-FORGE-6** : La résolution des dimanches du Temps Ordinaire applique la formule Segment I (`epiphania + 7·(N−1)`, avec correction pseudo-DOY) pour tout ordinal dont le DOY calculé est strictement inférieur au Mercredi des Cendres de l'année considérée. Pour tout autre ordinal, la formule Segment II (`adventus − 7·(35−N)`) s'applique. Le seuil est calculé en interne depuis `compute_easter(year)` — il n'est pas exposé dans l'`AnchorTable`.

---

## Références

- `liturgical-scheme.md` v1.3.3 — §3.2 : règle de résolution `tempus_ordinarium`
- `specification.md` v2.2 — Étape 2 : répertoire des ancres, ordre de résolution
- ADR-001 — Indépendance de la résolution DOY vis-à-vis de la logique saisonnière (INV-FORGE-4)
- ADR-002 — Interdiction du bloc `transfers` pour le calcul structurel (INV-FORGE-5)
- ADR-003 — Champ `ordinal` distinct de `offset` pour l'ancre `tempus_ordinarium` (V4a)
