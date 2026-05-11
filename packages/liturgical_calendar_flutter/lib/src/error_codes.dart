/// Codes de retour de l'Engine Rust (C-ABI).
///
/// Synchronisés avec `ffi.rs` de `liturgical-calendar-core`.
/// Toutes les fonctions FFI retournent [kalEngineOk] (0) en cas de succès
/// et une valeur strictement négative en cas d'erreur.
library;

/// Succès.
const int kalEngineOk = 0;

/// Un pointeur obligatoire passé à la fonction FFI était null.
const int kalErrNullPtr = -1;

/// Le buffer est trop court pour contenir un header valide.
const int kalErrBufTooSmall = -2;

/// Signature magic absente ou incorrecte (`b"KALD"` ou `b"LITS"`).
const int kalErrMagic = -3;

/// Version du format incompatible avec cet Engine.
const int kalErrVersion = -4;

/// SHA-256 du payload incorrect — données corrompues ou tronquées.
const int kalErrChecksum = -5;

/// Taille totale ou offsets internes incohérents avec le header.
const int kalErrFileSize = -6;

/// `year`, `doy`, `registry_index` ou `feast_id` hors de la plage couverte.
const int kalErrIndexOob = -7;

/// `secondary_offset` ou `count` dépassent les bornes du Secondary Pool.
const int kalErrPoolOob = -8;

/// Réservé — non émis en format v5.
const int kalErrReserved = -9;

/// Discriminant de layout incompatible (structure interne modifiée).
const int kalErrSchema = -10;

/// Incohérence `build_id` entre `.kald` et `.lits` — détecté côté Dart.
/// Valeur hors plage ABI Rust, propre au wrapper Flutter.
const int kalErrBuildIdMismatch = -22;
