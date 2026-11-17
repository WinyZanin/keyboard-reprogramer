mod actions;
mod hook;
mod keys;

use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;

use windows_sys::Win32::Foundation::{BOOL, TRUE};
use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, PostThreadMessageW, TranslateMessage, MSG, WM_QUIT,
};

use actions::ACTIONS;
use hook::Mapping;
use keys::KEYS;

/// Thread que roda o loop de mensagens (e o hook), para o Ctrl+C conseguir encerrá-la.
static MAIN_THREAD: AtomicU32 = AtomicU32::new(0);

/// Mostra as opções e lê um número válido do console. Retorna None se a entrada terminar.
fn choose<T>(title: &str, items: &'static [T], name: fn(&T) -> &'static str) -> Option<&'static T> {
    println!("\n{title}");
    for (i, item) in items.iter().enumerate() {
        println!(" {:>2}) {}", i + 1, name(item));
    }
    let stdin = io::stdin();
    loop {
        print!("> ");
        io::stdout().flush().ok();
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).ok()? == 0 {
            return None;
        }
        match line.trim().parse::<usize>() {
            Ok(n) if (1..=items.len()).contains(&n) => return Some(&items[n - 1]),
            _ => println!("Opção inválida. Digite um número de 1 a {}.", items.len()),
        }
    }
}

unsafe extern "system" fn ctrl_handler(_ctrl_type: u32) -> BOOL {
    PostThreadMessageW(MAIN_THREAD.load(Ordering::Relaxed), WM_QUIT, 0, 0);
    TRUE
}

fn main() {
    println!("=== Keyboard Reprogramer ===");

    let Some(key) = choose("Escolha a tecla a remapear:", KEYS, |k| k.name) else { return };
    let Some(action) = choose("Escolha a ação de mídia:", ACTIONS, |a| a.name) else { return };

    let (log_tx, log_rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in log_rx {
            println!("{line}");
        }
    });

    let _hook = match hook::install(Mapping { key, action }, log_tx) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Erro: {e}");
            std::process::exit(1);
        }
    };

    unsafe {
        MAIN_THREAD.store(GetCurrentThreadId(), Ordering::Relaxed);
        SetConsoleCtrlHandler(Some(ctrl_handler), TRUE);
    }

    println!(
        "\nAtivo: {} -> {}. Pressione Ctrl+C ou feche a janela para sair.",
        key.name, action.name
    );

    // Loop de mensagens: obrigatório para o hook de baixo nível receber eventos.
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    println!("Encerrando. A tecla {} voltou ao normal.", key.name);
    // `_hook` é desinstalado aqui (Drop -> UnhookWindowsHookEx).
}
