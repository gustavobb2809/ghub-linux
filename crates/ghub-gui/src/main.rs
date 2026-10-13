use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};
use ghub_gui::backend::{self, Mode, MODE};
use ghub_gui::client::single_instance::{self, Acquired};
use std::sync::Mutex;

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let smoke = arg_value(&args, "--smoke");
    let fake = arg_value(&args, "--fake");

    // `--smoke` sem nome: nunca cai para o modo de producao.
    if args.iter().any(|a| a == "--smoke") && smoke.as_deref().map_or(true, |s| s.is_empty() || s.starts_with("--")) {
        eprintln!("ghub-gui: --smoke exige o nome do cenario");
        std::process::exit(2);
    }
    // Cenarios sem backend (nao iniciam o Backend); os demais exigem --fake para nao tocar o barramento real.
    let no_backend = matches!(smoke.as_deref(), Some("theme" | "fail" | "load-failure"));
    if smoke.is_some() && fake.is_none() && !no_backend {
        eprintln!("ghub-gui: --smoke {} exige --fake <cenario>", smoke.as_deref().unwrap_or(""));
        std::process::exit(2);
    }
    let mode = match fake {
        #[cfg(feature = "fake-daemon")]
        Some(scenario) => Mode::Fake(scenario),
        #[cfg(not(feature = "fake-daemon"))]
        Some(_) => {
            eprintln!("ghub-gui: --fake exige um binario compilado com a feature fake-daemon (compilado sem fake-daemon)");
            std::process::exit(2);
        }
        None if smoke.is_some() => Mode::Idle,
        None => Mode::Real,
    };
    let _ = MODE.set(mode);

    // Smoke nao adquire a instancia unica (nao toca o barramento).
    let mut _primary = None;
    let qml = if let Some(name) = &smoke {
        if name == "load-failure" {
            // cenario de teste: QML inexistente deve encerrar com codigo 1
            "qrc:/qt/qml/org/ghublinux/gui/qml/DoesNotExist.qml"
        } else {
            "qrc:/qt/qml/org/ghublinux/gui/qml/Smoke.qml"
        }
    } else {
        match single_instance::acquire(single_instance::GUI_BUS_NAME, single_instance::GUI_PATH) {
            Ok(Acquired::Other) => std::process::exit(0),
            Ok(Acquired::Primary(mut p)) => {
                let rx = std::mem::replace(&mut p.activations, std::sync::mpsc::channel().1);
                let _ = backend::ACTIVATIONS.set(Mutex::new(Some(rx)));
                _primary = Some(p);
            }
            Err(e) => eprintln!("ghub-gui: sem instancia unica ({e}); seguindo mesmo assim"),
        }
        "qrc:/qt/qml/org/ghublinux/gui/qml/AppWindow.qml"
    };

    cxx_qt::init_crate!(cxx_qt_lib);
    cxx_qt::init_crate!(ghub_gui);
    cxx_qt::init_qml_module!("org.ghublinux.gui");
    QGuiApplication::set_desktop_file_name(&cxx_qt_lib::QString::from("org.ghublinux.Gui"));
    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();
    if let Some(mut engine) = engine.as_mut() {
        engine
            .as_mut()
            .on_object_creation_failed(|_, url| {
                eprintln!("ghub-gui: falha ao carregar o QML {}", url);
                std::process::exit(1);
            })
            .release();
        engine.load(&QUrl::from(qml));
    }
    let code = app.as_mut().map(|a| a.exec()).unwrap_or(1);
    // Destroi o motor (o Backend envia Quit) e espera o controller gravar a edicao pendente.
    drop(engine);
    // Libera o nome na sessao antes de esperar a gravacao pendente (um novo lancamento nao e engolido).
    drop(_primary);
    let handle = backend::CONTROLLER.lock().ok().and_then(|mut g| g.take());
    if let Some(h) = handle {
        let _ = h.join();
    }
    std::process::exit(code);
}
