fn main() {
    #[cfg(feature = "qt")]
    {
        use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};
        CxxQtBuilder::new_qml_module(QmlModule::new("org.ghublinux.gui").qml_files([
            QmlFile::from("qml/Theme.qml").singleton(true),
            QmlFile::from("qml/Smoke.qml"),
            QmlFile::from("qml/AppWindow.qml"),
        ]))
        .files(["src/backend.rs"])
        .build();

        // `src/backend.rs` (ponte cxx-qt) vive na LIB; o cxx-qt-build emite os
        // `rustc-link-lib` dos archives gerados (QML, inicializadores) para o
        // alvo lib, e o binario os herda via rlib. NAO religue esses archives
        // ao binario (gera "multiple definition"). So as libs Qt6 abaixo sao
        // repassadas ao binario; a lista e fixa (nao ha pkg-config/qmake):
        // atualize-a se passar a usar outros modulos Qt.
        for l in ["Qt6QuickControls2", "Qt6Qml", "Qt6Gui", "Qt6Core", "stdc++"] {
            println!("cargo::rustc-link-arg-bins=-l{l}");
        }
    }
}
