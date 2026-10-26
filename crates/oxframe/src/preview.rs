//! `preview` command: serves the project to the editor (see
//! `oxframe-preview`).

use oxframe_preview::server::{ServerOptions, serve};

use crate::cli::{EXIT_ENV, EXIT_ERROR, make_renderer, usage_error};
use crate::{Engine, PreviewAdapter, Project};

pub(crate) fn run(project: Project, args: Vec<String>) -> i32 {
    let mut opts = ServerOptions::default();
    let (mut gpu, mut cpu) = (false, false);
    let mut it = args.into_iter();
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--port" => match it.next().and_then(|v| v.parse().ok()) {
                Some(p) => opts.port = p,
                None => return usage_error("--port needs a number"),
            },
            "--gpu" => gpu = true,
            // The default since 0.21; still accepted, and wins over --gpu.
            "--cpu" => cpu = true,
            other => return usage_error(&format!("unknown option `{other}`")),
        }
    }
    let result = serve(
        move || {
            let engine = Engine::new(project).map_err(|e| e.to_string())?;
            let (renderer, backend) = make_renderer(gpu && !cpu);
            eprintln!("preview: rendering with {backend}");
            Ok(PreviewAdapter::new(engine, renderer))
        },
        opts,
    );
    match result {
        Ok(()) => 0,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            eprintln!("error: port in use: {e}");
            EXIT_ENV
        }
        Err(e) => {
            eprintln!("error: {e}");
            EXIT_ERROR
        }
    }
}
