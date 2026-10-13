use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

fn main() {
    cxx_qt::init_crate!(cxx_qt_lib);
    cxx_qt::init_qml_module!("org.ghublinux.gui");
    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/org/ghublinux/gui/qml/Smoke.qml"));
    }
    let code = app.as_mut().map(|a| a.exec()).unwrap_or(1);
    std::process::exit(code);
}
