use std::path::{Path, PathBuf};

use clap::Parser;
use liturgical_calendar_forge::{
    compile,
    parsing::ingest_corpus,
    variant_lock::VariantRegistryLock,
    I18nConfig,
};

// ── Arguments CLI ─────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name    = "kal-forge",
    version,
    about   = "Compile un scope liturgique YAML → .kald (+ .lits optionnel)",
)]
struct Args {
    /// Rite à compiler.
    #[arg(long, default_value = "romanus")]
    rite: String,

    /// Scope à compiler. Si absent, compile tous les scopes détectés dans --corpus.
    #[arg(long, default_value = "universale")]
    scope: String,

    /// Racine du corpus YAML.
    #[arg(long, default_value = "./corpus")]
    corpus: PathBuf,

    /// Répertoire de sortie des artefacts. Par défaut : ./artifacts.
    #[arg(long, default_value = "./artifacts")]
    out: PathBuf,

    /// Inclure les scopes marqués DRAFT (ignorés par défaut).
    #[arg(long, default_value_t = false)]
    include_drafts: bool,

    /// Racine i18n. Si fourni, produit un .lits par langue découverte.
    #[arg(long)]
    i18n: Option<PathBuf>,
}

// ── Point d'entrée ────────────────────────────────────────────────────────────

fn main() {
    let args = Args::parse();

    if let Err(e) = run(&args) {
        eprintln!("[kal-forge] erreur : {e}");
        std::process::exit(1);
    }
}

fn run(args: &Args) -> Result<(), liturgical_calendar_forge::ForgeError> {
    // ── Résolution des chemins ────────────────────────────────────────────────
    let rite_root   = args.corpus.join(&args.rite);
    let corpus_root = rite_root.join(&args.scope);
    let lock_path   = rite_root.join("feast_registry.lock");

    if !corpus_root.exists() {
        eprintln!(
            "[kal-forge] scope introuvable : {}",
            corpus_root.display()
        );
        std::process::exit(1);
    }

    // ── Détection sentinelle DRAFT ────────────────────────────────────────────
    if !args.include_drafts && corpus_root.join("DRAFT").exists() {
        eprintln!(
            "[kal-forge] scope ignoré (DRAFT) : {}/{}  — utilisez --include-drafts pour forcer",
            args.rite, args.scope
        );
        std::process::exit(0);
    }

    // ── Chargement du variant_registry.lock ───────────────────────────────────
    // Le lock réside à la racine du rite (partagé entre tous les scopes du rite).
    let lock_path = args.corpus.join(&args.rite).join("variant_registry.lock");
    let mut variant_lock = VariantRegistryLock::load(&lock_path)?;

    let scope_key  = format!("{}/{}", args.rite, args.scope);
    let variant_id = variant_lock.allocate(&scope_key)?;
    variant_lock.save(&lock_path)?;

    eprintln!(
        "[kal-forge] scope={scope_key}  variant_id={variant_id:#06x}"
    );

    // ── Ingest corpus ─────────────────────────────────────────────────────────
    let registry = ingest_corpus(&corpus_root)?;
    eprintln!("[kal-forge] {} fêtes chargées", registry.len());

    // ── Résolution chemin de sortie ───────────────────────────────────────────
    // Nom aplati : "romanus/nationalia/FR" → "romanus_nationalia_FR"
    // .kald : artifacts/romanus_nationalia_FR.kald
    // .lits : artifacts/romanus_nationalia_FR_la.lits  (préfixe + code langue)
    let flat_name = scope_key.replace('/', "_");
    let kald_path = args.out.join(format!("{flat_name}.kald"));

    std::fs::create_dir_all(&args.out)
        .map_err(liturgical_calendar_forge::ForgeError::Io)?;

    // ── i18n config (optionnel) ───────────────────────────────────────────────
    // lits_dir = args.out — les .lits sont écrits à plat avec un nom préfixé.
    // write_lits produit "{lang}.lits" dans lits_dir ; on renomme ensuite
    // en "{flat_name}_{lang}.lits" pour éviter toute collision inter-scopes.
    let i18n_config = match &args.i18n {
        Some(i18n_root) => {
            Some(I18nConfig {
                i18n_root: i18n_root.as_path(),
                lits_dir:  args.out.as_path(),
            })
        }
        None => None,
    };

    // ── Compilation ───────────────────────────────────────────────────────────
    let checksum = compile(registry, &kald_path, variant_id, i18n_config, &lock_path)?;

    let build_id = u64::from_le_bytes(checksum[..8].try_into().unwrap());
    eprintln!("[kal-forge] ✓  {}", kald_path.display());
    eprintln!("[kal-forge]    build_id = {build_id:#018x}");

    // ── Renommage des .lits : {lang}.lits → {flat_name}_{lang}.lits ──────────
    // write_lits produit "{lang}.lits" dans args.out. On renomme ici pour
    // obtenir le flat layout : romanus_universale_la.lits, etc.
    if args.i18n.is_some() {
        for entry in std::fs::read_dir(&args.out).map_err(liturgical_calendar_forge::ForgeError::Io)? {
            let entry = entry.map_err(liturgical_calendar_forge::ForgeError::Io)?;
            let path  = entry.path();
            if path.extension().map(|e| e == "lits").unwrap_or(false) {
                let lang = path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();
                // Ne renommer que les .lits sans préfixe (ceux qu'on vient de produire).
                if !lang.contains('_') {
                    let new_name = format!("{flat_name}_{lang}.lits");
                    let new_path = args.out.join(&new_name);
                    std::fs::rename(&path, &new_path)
                        .map_err(liturgical_calendar_forge::ForgeError::Io)?;
                    eprintln!("[kal-forge] ✓  {}", new_path.display());
                }
            }
        }
    }

    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Scanne `corpus/<rite>/` et retourne la liste des scope_keys détectés,
/// en excluant les entrées non-répertoire et les scopes DRAFT (sauf `include_drafts`).
///
/// Retourne des chemins normalisés `"<rite>/<scope>"`.
/// Utilisé par le mode "compile tout" (scope absent en argv — non implémenté ici,
/// prévu pour une prochaine itération).
#[allow(dead_code)]
fn discover_scopes(
    corpus:         &Path,
    rite:           &str,
    include_drafts: bool,
) -> std::io::Result<Vec<String>> {
    let rite_dir = corpus.join(rite);
    let mut scopes = Vec::new();

    for entry in std::fs::read_dir(&rite_dir)? {
        let entry = entry?;
        let path  = entry.path();

        if !path.is_dir() {
            continue;
        }

        // Exclure variant_registry.lock et fichiers parasites
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_owned(),
            None    => continue,
        };

        if !include_drafts && path.join("DRAFT").exists() {
            continue;
        }

        scopes.push(format!("{rite}/{name}"));
    }

    scopes.sort(); // ordre déterministe
    Ok(scopes)
}
