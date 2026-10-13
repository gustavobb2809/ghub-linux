fn main() {
    #[cfg(feature = "qt")]
    {
        use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};
        CxxQtBuilder::new_qml_module(QmlModule::new("org.ghublinux.gui").qml_files([
            QmlFile::from("qml/Theme.qml").singleton(true),
            QmlFile::from("qml/Smoke.qml"),
        ]))
        .build();

        // O cargo so repassa os `-l` deste build script ao alvo lib (o binario
        // nao depende da lib). Ligamos tudo ao binario aqui: o modulo QML
        // gerado e os inicializadores precisam de whole-archive porque
        // registram recursos/plugin por inicializadores estaticos.
        let out = std::env::var("OUT_DIR").unwrap();
        for lib in [
            "cxx-qt-call-init-qml_module_org_ghublinux_gui",
            "cxx-qt-call-init-crate_ghub_gui",
            "ghub-gui-cxxqt-generated",
        ] {
            println!(
                "cargo::rustc-link-arg-bins=-Wl,--whole-archive,{out}/lib{lib}.a,--no-whole-archive"
            );
        }
        for l in ["Qt6QuickControls2", "Qt6Qml", "Qt6Gui", "Qt6Core", "stdc++"] {
            println!("cargo::rustc-link-arg-bins=-l{l}");
        }
    }
}
