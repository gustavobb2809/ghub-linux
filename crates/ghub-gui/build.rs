fn main() {
    #[cfg(feature = "qt")]
    {
        use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};
        CxxQtBuilder::new_qml_module(QmlModule::new("org.ghublinux.gui").qml_files([
            QmlFile::from("qml/Theme.qml").singleton(true),
            QmlFile::from("qml/Smoke.qml"),
            QmlFile::from("qml/AppWindow.qml"),
            QmlFile::from("qml/components/Card.qml"),
            QmlFile::from("qml/components/PillButton.qml"),
            QmlFile::from("qml/components/NavTab.qml"),
            QmlFile::from("qml/components/Toast.qml"),
            QmlFile::from("qml/components/Tag.qml"),
            QmlFile::from("qml/components/Toggle.qml"),
            QmlFile::from("qml/components/Glyph.qml"),
            QmlFile::from("qml/components/SideEntry.qml"),
            QmlFile::from("qml/components/DpiSlider.qml"),
            QmlFile::from("qml/components/SettingRow.qml"),
            QmlFile::from("qml/components/MouseArt.qml"),
            QmlFile::from("qml/components/Logo.qml"),
            QmlFile::from("qml/pages/Overview.qml"),
            QmlFile::from("qml/pages/Sensitivity.qml"),
            QmlFile::from("qml/pages/Settings.qml"),
            QmlFile::from("qml/pages/Disconnected.qml"),
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
