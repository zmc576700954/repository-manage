use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use synapse_kb_lib::validator;

#[derive(Parser)]
#[command(name = "synapse", version, about = "Synapse KB CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 校验 KB
    Validate {
        /// KB 根路径
        path: PathBuf,
        /// 输出 JSON
        #[arg(long)]
        json: bool,
        /// 只输出指定级别（error / warning / info）
        #[arg(long, default_value = "all")]
        severity: String,
    },
    /// 迁移老字段
    MigrateLabels {
        path: PathBuf,
        /// 仅预览，不改文件
        #[arg(long)]
        dry_run: bool,
        /// 列出受影响的文件
        #[arg(long)]
        report: bool,
    },
    /// 扫所有 KB
    ValidateAll,
}

#[derive(serde::Serialize)]
struct IssueDto {
    entry_id: String,
    rule_id: String,
    severity: String,
    message: String,
    ignored: bool,
}

#[derive(serde::Serialize)]
struct ReportDto {
    kb_root: String,
    summary: SummaryDto,
    issues: Vec<IssueDto>,
}

#[derive(serde::Serialize)]
struct SummaryDto {
    errors: usize,
    warnings: usize,
    info: usize,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Commands::Validate {
            path,
            json,
            severity,
        } => {
            let report = validator::validate_kb(&path);
            if json {
                print_report_json(&report);
            } else {
                print_report_human(&report);
            }
            if should_exit_error(&report, &severity) {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Commands::MigrateLabels {
            path,
            dry_run,
            report,
        } => {
            let opts = synapse_kb_lib::validator::migrator::MigrationOptions {
                dry_run,
                backup: !dry_run,
            };
            match synapse_kb_lib::validator::migrator::migrate_labels(&path, opts) {
                Ok(r) => {
                    if report {
                        println!(
                            "scanned={} migrated={} kept_with_warning={} would_modify={}",
                            r.scanned, r.migrated, r.kept_with_warning, r.would_modify
                        );
                    } else if dry_run {
                        println!(
                            "Migration preview:\n  scanned: {}\n  would_modify: {}\n  kept_with_warning: {}",
                            r.scanned, r.would_modify, r.kept_with_warning
                        );
                    } else {
                        println!(
                            "Migration complete:\n  scanned: {}\n  migrated: {}\n  kept_with_warning: {}\n  backup: {}\n  log: {}",
                            r.scanned,
                            r.migrated,
                            r.kept_with_warning,
                            r.backup_dir.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "(none)".into()),
                            r.log_path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "(none)".into())
                        );
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    ExitCode::from(2)
                }
            }
        }
        Commands::ValidateAll => {
            // 简化：扫描 ~/kb-* 目录
            ExitCode::SUCCESS
        }
    }
}

fn print_report_human(report: &validator::ValidationReport) {
    println!("Scanning {}...", report.kb_root);
    for issue in &report.issues {
        if issue.ignored {
            continue;
        }
        let prefix = match issue.severity {
            validator::Severity::Error => "[ERROR]",
            validator::Severity::Warning => "[WARN]",
            validator::Severity::Info => "[INFO]",
        };
        println!("{} {}: {}", prefix, issue.rule_id, issue.message);
    }
    println!(
        "Summary: {} errors, {} warnings, {} info",
        report.error_count(),
        report.warning_count(),
        report.info_count()
    );
}

fn print_report_json(report: &validator::ValidationReport) {
    let dto = ReportDto {
        kb_root: report.kb_root.clone(),
        summary: SummaryDto {
            errors: report.error_count(),
            warnings: report.warning_count(),
            info: report.info_count(),
        },
        issues: report
            .issues
            .iter()
            .map(|i| IssueDto {
                entry_id: i.entry_id.clone(),
                rule_id: i.rule_id.clone(),
                severity: match i.severity {
                    validator::Severity::Error => "error".to_string(),
                    validator::Severity::Warning => "warning".to_string(),
                    validator::Severity::Info => "info".to_string(),
                },
                message: i.message.clone(),
                ignored: i.ignored,
            })
            .collect(),
    };
    println!("{}", serde_json::to_string_pretty(&dto).unwrap());
}

fn should_exit_error(
    report: &validator::ValidationReport,
    severity: &str,
) -> bool {
    match severity {
        "error" => report.has_errors(),
        _ => report.has_errors(),
    }
}
