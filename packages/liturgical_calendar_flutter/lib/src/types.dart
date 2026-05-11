import 'ffi_structs.dart';

// ── Exceptions ────────────────────────────────────────────────────────────────

/// Erreur retournée par l'Engine lors d'une opération FFI.
///
/// [code] est l'un des [kalErr*] définis dans `error_codes.dart`.
class KalException implements Exception {
  final int code;
  const KalException(this.code);

  @override
  String toString() => 'KalException(code: $code)';
}

/// Échec de validation du buffer `.kald` (SHA-256, magic, version, schema…).
class KalValidationException extends KalException {
  const KalValidationException(super.code);

  @override
  String toString() => 'KalValidationException(code: $code)';
}

/// Incohérence entre le `.kald` et le `.lits` : `build_id` ne correspond pas.
///
/// Indique que les deux fichiers ne sont pas issus de la même compilation Forge.
class KalBuildIdMismatchException extends KalException {
  const KalBuildIdMismatchException() : super(-22);

  @override
  String toString() =>
      'KalBuildIdMismatchException: '
      'lits.build_id != kald.checksum[0..8] — paire de fichiers incohérente';
}

// ── Énumérations ─────────────────────────────────────────────────────────────

/// Rang de préséance liturgique — bits [3:0] de [KalFeastEntry.flags].
///
/// Ordre numérique = priorité décroissante (0 = priorité absolue).
/// Valeurs 13–15 réservées système — non émises par le Forge v2.
enum Precedence {
  /// 1. Triduum pascal.
  triduumSacrum(0),

  /// 2. Solennités majeures : Nativité, Épiphanie, Ascension, Pentecôte.
  ///    Inclut dimanches d'Avent, Carême, Temps Pascal, Mercredi des Cendres, Semaine Sainte.
  sollemnitatesMaiores(1),

  /// 3. Solennités du Seigneur, de la Vierge et des saints du calendrier général.
  ///    Inclut la Commémoration des fidèles défunts.
  sollemnitatesGenerales(2),

  /// 4. Solennités propres (patron du lieu, dédicace de l'église, titulaire de l'ordre).
  sollemnitatesPropria(3),

  /// 5. Fêtes du Seigneur inscrites au calendrier général.
  festaDomini(4),

  /// 6. Dimanches du temps de Noël et dimanches du temps ordinaire.
  dominicaePerAnnum(5),

  /// 7. Fêtes de la Vierge Marie et des saints du calendrier général.
  festaBMVEtSanctorumGenerales(6),

  /// 8. Fêtes propres (patron du diocèse, anniversaire dédicace cathédrale, etc.).
  festaPropria(7),

  /// 9. Féries privilégiées : Avent (17-24 déc.), Octave de Noël, féries de Carême.
  feriaePrivilegiatae(8),

  /// 10. Mémoires obligatoires du calendrier général.
  memoriaeObligatoriaGenerales(9),

  /// 11. Mémoires obligatoires propres (patron du lieu, diocèse ou ordre).
  memoriaeObligatoriaePropria(10),

  /// 12. Mémoires facultatives.
  memoriaeAdLibitum(11),

  /// 13. Féries communes : Avent (jusqu'au 16 déc.), Noël, Temps Pascal et Temps Ordinaire.
  feriaePerAnnum(12);

  final int value;
  const Precedence(this.value);

  /// Retourne `null` pour les valeurs réservées 13–15 (forward-compat).
  static Precedence? fromValue(int v) {
    for (final e in Precedence.values) {
      if (e.value == v) return e;
    }
    return null;
  }
}

/// Couleur liturgique — bits [7:4] de [KalFeastEntry.flags].
///
/// Intentionnellement nommé [LiturgicalColor] pour éviter la collision
/// avec [dart:ui.Color] et [package:flutter/material.dart].
/// La valeur 6 est réservée (usage liturgique futur : or/argent).
enum LiturgicalColor {
  /// Blanc — fêtes du Seigneur, Vierge, Confesseurs, Docteurs.
  albus(0),

  /// Rouge — Passion, Apôtres, Martyrs, Pentecôte.
  rubeus(1),

  /// Vert — temps ordinaire.
  viridis(2),

  /// Violet — Avent, Carême.
  violaceus(3),

  /// Rose — Gaudete (Avent III), Laetare (Carême IV).
  rosaceus(4),

  /// Noir — messes des défunts.
  niger(5);

  final int value;
  const LiturgicalColor(this.value);

  static LiturgicalColor? fromValue(int v) {
    for (final e in LiturgicalColor.values) {
      if (e.value == v) return e;
    }
    return null;
  }
}

/// Période du calendrier liturgique — bits [10:8] de [KalFeastEntry.flags].
///
/// La valeur 7 est réservée.
enum LiturgicalPeriod {
  /// Temps ordinaire (valeur par défaut).
  tempusOrdinarium(0),

  /// Avent.
  tempusAdventus(1),

  /// Temps de Noël.
  tempusNativitatis(2),

  /// Carême.
  tempusQuadragesimae(3),

  /// Triduum Pascal.
  triduumPaschale(4),

  /// Temps pascal.
  tempusPaschale(5),

  /// Semaine Sainte (Rameaux inclus → Mercredi Saint inclus).
  /// Subdivision opérationnelle du Carême.
  diesSancti(6);

  final int value;
  const LiturgicalPeriod(this.value);

  static LiturgicalPeriod? fromValue(int v) {
    for (final e in LiturgicalPeriod.values) {
      if (e.value == v) return e;
    }
    return null;
  }
}

/// Nature d'une célébration — bits [13:11] de [KalFeastEntry.flags].
///
/// Les valeurs 6–7 sont réservées.
enum Nature {
  /// Solennité.
  sollemnitas(0),

  /// Fête.
  festum(1),

  /// Dimanche du temps ordinaire.
  dominica(2),

  /// Mémoire (obligatoire ou facultative).
  memoria(3),

  /// Commémoration — trace d'une célébration déclassée.
  commemoratio(4),

  /// Férie.
  feria(5);

  final int value;
  const Nature(this.value);

  static Nature? fromValue(int v) {
    for (final e in Nature.values) {
      if (e.value == v) return e;
    }
    return null;
  }
}

// ── Data classes ─────────────────────────────────────────────────────────────

/// Données d'occurrence journalière — copie Dart de [KalTimelineEntry].
///
/// Immutable. Créé par [LiturgicalCalendar.getDay] via [_fromFfi].
class TimelineEntryData {
  /// Index 1-based dans le Feast Registry. 0 = Padding Entry.
  final int primaryIndex;

  /// Offset en u16 dans le Secondary Pool.
  final int secondaryOffset;

  /// Nombre de célébrations secondaires pour ce slot.
  final int secondaryCount;

  /// `true` si des premières vêpres sont célébrées la veille.
  final bool hasVesperaeI;

  /// `true` si une messe de vigile est prévue.
  final bool hasVigilia;

  const TimelineEntryData({
    required this.primaryIndex,
    required this.secondaryOffset,
    required this.secondaryCount,
    required this.hasVesperaeI,
    required this.hasVigilia,
  });

  /// `true` si ce slot ne porte aucune célébration (primaryIndex == 0).
  bool get isPadding => primaryIndex == 0;

  factory TimelineEntryData._fromFfi(KalTimelineEntry e) => TimelineEntryData(
        primaryIndex:    e.primaryIndex,
        secondaryOffset: e.secondaryOffset,
        secondaryCount:  e.secondaryCount,
        hasVesperaeI:    (e.occurrenceFlags & 0x01) != 0,
        hasVigilia:      (e.occurrenceFlags & 0x02) != 0,
      );

  @override
  String toString() => 'TimelineEntryData('
      'primaryIndex: $primaryIndex, '
      'secondaries: $secondaryCount'
      '${hasVesperaeI ? ", vesperaeI" : ""}'
      '${hasVigilia ? ", vigilia" : ""}'
      ')';
}

/// Invariants d'une fête — copie Dart de [KalFeastEntry].
///
/// Immutable. Les champs enum peuvent être `null` si la valeur de bit field
/// n'est pas reconnue (forward-compat avec les formats futurs).
class FeastEntryData {
  final int feastId;

  /// Valeur brute de `flags` — utile si un champ enum est `null`.
  final int flagsRaw;

  final Precedence?       precedence;
  final LiturgicalColor?  color;
  final LiturgicalPeriod? period;
  final Nature?           nature;

  /// `true` si une messe de vigile propre est associée à cette fête.
  final bool hasVigilMass;

  const FeastEntryData({
    required this.feastId,
    required this.flagsRaw,
    required this.precedence,
    required this.color,
    required this.period,
    required this.nature,
    required this.hasVigilMass,
  });

  factory FeastEntryData._fromFfi(KalFeastEntry e) {
    final f = e.flags;
    return FeastEntryData(
      feastId:      e.feastId,
      flagsRaw:     f,
      precedence:   Precedence.fromValue(f & 0x000F),
      color:        LiturgicalColor.fromValue((f >> 4) & 0x000F),
      period:       LiturgicalPeriod.fromValue((f >> 8) & 0x0007),
      nature:       Nature.fromValue((f >> 11) & 0x0007),
      hasVigilMass: (f & (1 << 14)) != 0,
    );
  }

  @override
  String toString() => 'FeastEntryData('
      'feastId: $feastId, '
      'precedence: $precedence, '
      'color: $color, '
      'nature: $nature'
      ')';
}

/// Résultat d'un appel [LiturgicalCalendar.getDay].
class DayData {
  /// Métadonnées d'occurrence (Timeline).
  final TimelineEntryData timeline;

  /// Invariants de la fête principale.
  /// `null` si [TimelineEntryData.isPadding] est `true`.
  final FeastEntryData? primary;

  /// Registry indices (1-based) des célébrations secondaires.
  ///
  /// Liste immuable. Appeler [LiturgicalCalendar.getFeast] pour résoudre
  /// chaque index en [FeastEntryData].
  final List<int> secondaryIndices;

  const DayData({
    required this.timeline,
    required this.primary,
    required this.secondaryIndices,
  });

  @override
  String toString() => 'DayData('
      'primary: $primary, '
      'secondaries: ${secondaryIndices.length}'
      ')';
}

/// Label et annotation d'une fête depuis un buffer `.lits`.
///
/// Les `String` sont construites par copie depuis le buffer natif
/// lors de l'appel [LiturgicalCalendar.getLabel].
class LitsEntryData {
  /// Titre officiel de la fête pour la langue et l'année demandées.
  final String label;

  /// Précision liturgique ou titre alternatif. `null` si absent.
  final String? annotation;

  const LitsEntryData({required this.label, this.annotation});

  @override
  String toString() => 'LitsEntryData('
      'label: "$label"'
      '${annotation != null ? ', annotation: "$annotation"' : ''}'
      ')';
}
