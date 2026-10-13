//! Biblioteca sem Qt da GUI ghub (modelo e cliente do daemon).

// A build.rs do cxx-qt emite os `rustc-link-lib` apenas para o alvo lib; o
// binario e os testes da lib so os recebem se as crates Qt forem realmente
// ligadas, entao forcamos isso aqui.
#[cfg(feature = "qt")]
extern crate cxx;
#[cfg(feature = "qt")]
extern crate cxx_qt;
#[cfg(feature = "qt")]
extern crate cxx_qt_lib;
