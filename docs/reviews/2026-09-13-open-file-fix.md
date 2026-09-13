# Open file su Linux — diagnosi e correzione

Il comando estraeva correttamente il file, ma `open::that` chiamava `xdg-open`. Su questa macchina il suo percorso generico esegue `nvim` direttamente, ignorando `Terminal=true` del desktop entry. Con gli stream standard di un'app GUI, l'editor rimane invisibile e il comando resta in attesa.

Riprodotto con un TXT sintetico e `xdg-open` con stdin/stdout/stderr scollegati: timeout dopo tre secondi e processo `nvim` senza TTY. Lo stesso TXT, aperto tramite GIO, genera una finestra WezTerm e il launcher termina immediatamente. Verificato anche tramite la funzione Rust corretta.

`file_open::launch` usa ora GIO su Linux, già presente nello stack GTK dell'app. Il percorso viene convertito in URI tramite GLib; l'associazione desktop e l'apertura del terminale vengono gestite da GIO. Gli errori continuano a raggiungere il messaggio del pannello dettagli. Le altre piattaforme mantengono il launcher precedente.

Il test `terminal_association_is_launched_in_a_terminal` usa associazioni MIME temporanee e un processo isolato, senza modificare le preferenze dell'utente. Verifica che `Terminal=true` invii il comando al terminale e che un nome contenente spazi e simboli rimanga un unico argomento. Prima della correzione falliva con `Terminal=true was ignored: editor launched without a terminal`; dopo passa.

```bash
cargo test -p dedup-app file_open::tests::terminal_association_is_launched_in_a_terminal -- --nocapture
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p dedup-app
```

La build aggiornata è `/.cargo-targets/targets/dedup/main/target/debug/dedup-app`. Un processo già aperto continua a usare il codice precedente fino al riavvio.

## Visibilità di Open file

Il pannello verifica le associazioni desktop tramite `can_open_file`. Su Linux GIO identifica il tipo dal nome, senza estrarre o decomprimere il contenuto, e cerca l'app predefinita. Un risultato negativo nasconde il pulsante e mostra una spiegazione. Durante la verifica viene mostrato lo stato di caricamento. Una classificazione incerta, un errore nel controllo o una piattaforma senza verifica implementata restituiscono uno stato sconosciuto: l'apertura resta disponibile, evitando di dichiarare non supportato un file che il sistema potrebbe riconoscere dal contenuto.

Le risposte tardive vengono ignorate al cambio del file selezionato. Il test Rust verifica anche che `.log` e `.txt` siano riconosciuti con un editor associato. La suite UI conta ora 13 test superati e comprende associazione assente, controllo fallito, errore di apertura e risposte tardive. Verificati anche 9 test di dedup-app, Svelte check, Clippy e build frontend/desktop.
