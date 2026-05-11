/// Engine de calendrier liturgique pour Flutter.
///
/// Expose les types principaux. Les structs FFI internes ([KalTimelineEntry],
/// [KalFeastEntry], [KalHeader]) ne sont pas exportées — elles sont un détail
/// d'implémentation.
///
/// ## Usage minimal
///
/// ```dart
/// import 'package:liturgical_calendar_flutter/liturgical_calendar_flutter.dart';
///
/// final cal = LiturgicalCalendar();
///
/// // Charger les données — typiquement depuis rootBundle :
/// // final kald = (await rootBundle.load('assets/calendar.kald'))
/// //     .buffer.asUint8List();
/// // final lits = (await rootBundle.load('assets/calendar.lits'))
/// //     .buffer.asUint8List();
/// cal.load(kald, lits);
///
/// final day = cal.getDay(2024, 12, 25); // Noël
/// if (day != null && !day.timeline.isPadding) {
///   final label = cal.getLabel(day.primary!.feastId, 2024);
///   print(label?.label); // "Nativitas Domini Nostri Iesu Christi"
/// }
///
/// cal.dispose();
/// ```
library;

export 'src/calendar.dart' show LiturgicalCalendar;
export 'src/error_codes.dart';
export 'src/types.dart'
    show
        DayData,
        FeastEntryData,
        KalBuildIdMismatchException,
        KalException,
        KalValidationException,
        LiturgicalColor,
        LiturgicalPeriod,
        LitsEntryData,
        Nature,
        Precedence,
        TimelineEntryData;
