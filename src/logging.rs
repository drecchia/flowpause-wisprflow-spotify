/// Imprime uma linha com o horário local, por exemplo `[14:03:07.512] ditado iniciou`.
macro_rules! logln {
    ($($arg:tt)*) => {
        println!(
            "[{}] {}",
            chrono::Local::now().format("%H:%M:%S%.3f"),
            format_args!($($arg)*)
        )
    };
}
