//! Synthetic JSON test-data generator for BENCHMARK.md — not part of jsoned itself, dev-only.
//! Run: `cargo run --example json_perf_gen -- --count 50000 --format json --output big.json`

use clap::Parser;
use fake::{faker::{internet::en::SafeEmail, lorem::en::Words, name::en::Name}, Fake};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;
use std::fs::File;
use std::io::{BufWriter, Write};
use uuid::Uuid;

#[derive(clap::ValueEnum, Clone, Debug)]
enum OutputFormat {
    /// Un objet JSON par ligne (non chargeable par jsoned pour l'instant)
    Jsonl,
    /// Un unique tableau JSON — chargeable directement dans jsoned dès aujourd'hui
    Json,
}

/// CLI pour générer des fichiers JSON massifs pour les tests de performance
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Nombre d'enregistrements JSON à générer
    #[arg(short, long, default_value_t = 1000)]
    count: usize,

    /// Nom du fichier de sortie (par défaut : perf_test_data.jsonl ou .json selon --format)
    #[arg(short, long)]
    output: Option<String>,

    /// Format de sortie : jsonl (un objet par ligne) ou json (un tableau unique,
    /// seul format que jsoned sait ouvrir aujourd'hui — utile pour valider le
    /// lazy flatten en conditions réelles avant que le support JSONL n'arrive)
    #[arg(short, long, value_enum, default_value_t = OutputFormat::Jsonl)]
    format: OutputFormat,
}

// Structure représentant le payload "professionnel" de test
#[derive(Serialize)]
struct TestRecord {
    metadata: Metadata,
    payload: Payload,
}

#[derive(Serialize)]
struct Metadata {
    // Index séquentiel (0-based) — permet de cibler à coup sûr le premier, le
    // milieu ou le dernier enregistrement pour vérifier que la latence d'édition
    // du lazy flatten reste plate quelle que soit la position dans le document.
    index: usize,
    transaction_id: String,
    generated_at: String,
    version: &'static str,
}

#[derive(Serialize)]
struct Payload {
    user_id: String,
    full_name: String,
    email: String,
    tier: &'static str,
    tags: Vec<String>,
    metrics: PerformanceMetrics,
}

#[derive(Serialize)]
struct PerformanceMetrics {
    request_count: u32,
    is_active: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let output = args.output.clone().unwrap_or_else(|| match args.format {
        OutputFormat::Jsonl => "perf_test_data.jsonl".to_string(),
        OutputFormat::Json => "perf_test_data.json".to_string(),
    });

    println!("🚀 Démarrage de la génération de {} enregistrements...", args.count);

    // Utilisation de BufWriter pour des performances d'écriture optimales
    let file = File::create(&output)?;
    let mut writer = BufWriter::new(file);

    // Barre de progression pour le confort visuel en CLI
    let pb = ProgressBar::new(args.count as u64);
    pb.set_style(
        ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({eta})")
            .unwrap()
            .progress_chars("#>-")
    );

    let tiers = ["free", "premium", "enterprise", "vip"];

    // En mode "json", tout le fichier est un unique tableau : on écrit le crochet
    // ouvrant et une virgule avant chaque élément sauf le premier — toujours en
    // streaming ligne par ligne, sans jamais garder tous les enregistrements en
    // mémoire en même temps.
    if matches!(args.format, OutputFormat::Json) {
        writeln!(writer, "[")?;
    }

    for i in 0..args.count {
        // Sélection pseudo-aléatoire du tier
        let tier = tiers[i % tiers.len()];

        // Génération des données factices avec Faker et Uuid
        let record = TestRecord {
            metadata: Metadata {
                index: i,
                transaction_id: format!("tx-perf-{}", Uuid::new_v4()),
                generated_at: chrono::Utc::now().to_rfc3339(),
                version: "1.2.0",
            },
            payload: Payload {
                user_id: format!("usr_{}", Uuid::new_v4().simple()),
                full_name: Name().fake(),
                email: SafeEmail().fake(),
                tier,
                tags: Words(1..5).fake(),
                metrics: PerformanceMetrics {
                    request_count: (i as u32 % 500) + 1,
                    is_active: i % 2 == 0,
                },
            },
        };

        // Sérialisation en une seule ligne (minifiée)
        let json_line = serde_json::to_string(&record)?;

        match args.format {
            OutputFormat::Jsonl => {
                // Un objet JSON par ligne (Format JSON Lines)
                writeln!(writer, "{}", json_line)?;
            }
            OutputFormat::Json => {
                // Élément de tableau : virgule de séparation avant tous sauf le premier
                if i > 0 {
                    writeln!(writer, ",{}", json_line)?;
                } else {
                    writeln!(writer, "{}", json_line)?;
                }
            }
        }

        if i % 1000 == 0 {
            pb.set_position(i as u64);
        }
    }

    if matches!(args.format, OutputFormat::Json) {
        writeln!(writer, "]")?;
    }

    // On s'assure que tout le buffer est bien écrit sur le disque
    writer.flush()?;
    pb.finish_with_message("Terminé !");

    println!("✨ Fichier généré avec succès : {}", output);
    Ok(())
}
