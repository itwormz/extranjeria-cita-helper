fn destination(path: &std::path::Path) -> Result<std::path::PathBuf, &'static str> {
    if path.exists() {
        return path.canonicalize().map_err(|_| "cannot resolve path");
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    let name = path.file_name().ok_or("invalid file path")?;
    Ok(parent
        .canonicalize()
        .map_err(|_| "cannot resolve parent directory")?
        .join(name))
}
// Developer-only offline replay; no native browser or credential-store entry point.
use std::{
    io::{Read, Write},
    path::PathBuf,
};
fn execute() -> Result<(), &'static str> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("replay")) {
        return Err("usage: ceh-cli replay FILE [--database FILE] [--output FILE]");
    }
    let input = PathBuf::from(args.next().ok_or("missing replay file")?);
    let input_identity = destination(&input)?;
    let mut database = None;
    let mut output = None;
    while let Some(option) = args.next() {
        let target = match option.to_str() {
            Some("--database") => &mut database,
            Some("--output") => &mut output,
            _ => return Err("unknown option"),
        };
        if target.is_some() {
            return Err("duplicate option");
        }
        *target = Some(PathBuf::from(args.next().ok_or("missing option value")?));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&input)
        .map_err(|_| "cannot read replay")?
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read replay")?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("replay exceeds size limit");
    }
    let spec: ceh_cli::replay::ReplaySpec =
        serde_json::from_slice(&bytes).map_err(|_| "invalid replay document")?;
    let temporary = tempfile::tempdir().map_err(|_| "cannot create isolated storage")?;
    let database = database.unwrap_or_else(|| temporary.path().join("replay.sqlite"));
    let database_identity = destination(&database)?;
    if database_identity == input_identity {
        return Err("database must differ from input");
    }
    if let Some(output) = &output {
        let identity = destination(output)?;
        if identity == database_identity || identity == input_identity {
            return Err("output must differ from input and database");
        }
    }
    ceh_store::migrate::migrate_paths(
        &ceh_store::migrate::MigrationOptions {
            database: database.clone(),
            legacy_json: None,
            backup_directory: temporary.path().join("backups"),
        },
        spec.wall_start_millis,
    )
    .map_err(|_| "cannot initialize replay storage")?;
    let mut db = rusqlite::Connection::open(database).map_err(|_| "cannot open replay storage")?;
    db.busy_timeout(std::time::Duration::from_millis(100))
        .map_err(|_| "cannot configure replay storage")?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .map_err(|_| "cannot initialize replay runtime")?;
    let report = runtime
        .block_on(ceh_cli::replay::run(spec, &mut db))
        .map_err(|_| "invalid replay sequence")?;
    let bytes = serde_json::to_vec_pretty(&report).map_err(|_| "cannot encode replay report")?;
    if let Some(output) = output {
        let parent = output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        let mut file =
            tempfile::NamedTempFile::new_in(parent).map_err(|_| "cannot prepare output")?;
        file.write_all(&bytes).map_err(|_| "cannot write output")?;
        file.as_file()
            .sync_all()
            .map_err(|_| "cannot sync output")?;
        file.persist(output).map_err(|_| "cannot save output")?;
    } else {
        std::io::stdout()
            .lock()
            .write_all(&bytes)
            .map_err(|_| "cannot write report")?;
    }
    Ok(())
}
fn main() {
    if let Err(message) = execute() {
        eprintln!("{message}");
        std::process::exit(2);
    }
}
