# Implementazione UI/UX — 13 settembre 2026

La UI ora separa preparazione, scansione e risultato. Durante la scansione mostra attività, file elaborati, dati, duplicati e tempo trascorso, con il comando di arresto visibile anche a 800×500. Il risultato rimane consultabile nella vista Activity durante la sessione. Non viene mostrata una percentuale senza un totale attendibile.

## Cambiamenti

- Navigazione laterale tra Files, Duplicates, Activity e Overview; selettore dell'archivio e azione Add folder sempre riconoscibili.
- Temi chiaro, scuro e di sistema, preferenza salvata, contrasto della selezione corretto, focus visibile e rispetto del movimento ridotto.
- Preparazione della scansione con percorso sorgente in primo piano e opzioni avanzate raccolte in una sezione espandibile.
- Distinzione tra scansione completata, interrotta e fallita; errori parziali e percorso del log consultabili. Un errore nell'aggiornamento degli archivi non cancella un risultato di scansione riuscito.
- Browser delle cartelle con breadcrumb, ricerca nella cartella corrente, ordinamento e pagine da 100 elementi. Navigazione conservata passando tra le viste.
- Vista duplicati con ricerca dei percorsi, pagine da 20 gruppi e percorsi espandibili; apertura diretta dei dettagli.
- Dettagli compatti con risposte asincrone obsolete scartate, errori espliciti e anteprime raster limitate a 12 MB. A finestra stretta il pannello occupa lo spazio della lista e offre un ritorno esplicito.
- Overview riferita all'archivio corrente e aggiornata al cambio di archivio.
- Creazione e apertura semplificate; rimozione dall'elenco con Undo. Rimuovere dall'elenco conserva il file dell'archivio su disco.
- Etichette dei campi associate ai controlli e finestre modali protette dalla chiusura durante le operazioni in corso.

La lingua dell'interfaccia rimane l'inglese, coerente con l'app esistente.

## Verifica

Eseguiti con successo:

```bash
pnpm -C app check
pnpm -C app build
PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH=/usr/bin/chromium pnpm -C app test:ui --workers=2
cargo build -p dedup-app
```

Il controllo Svelte restituisce zero errori e zero avvisi. I nove test Playwright verificano scansione e risultato, arresto, errori, aggiornamenti falliti, paginazione e ricerca, risposte obsolete, apertura dei duplicati, cambio archivio, Undo e primo avvio. Comprendono controlli axe WCAG A/AA sulle schermate principali e sul modulo di scansione, nei due temi. Non costituiscono una certificazione completa di accessibilità.

Compilato anche l'eseguibile desktop in profilo dev, con frontend incorporato. Su questa macchina si trova in `/.cargo-targets/targets/dedup/main/target/debug/dedup-app`; l'app installata non viene sostituita dalla compilazione.

I test eseguono il frontend reale in Chromium con IPC Tauri simulato: non misurano le prestazioni di scansione del motore Rust o del WebView nativo. Per usare il browser gestito da Playwright, eseguire `pnpm -C app exec playwright install chromium`, quindi `pnpm -C app test:ui` senza la variabile del browser di sistema.

## Schermate

Le immagini usano dati sintetici riproducibili tramite la fixture dei test.

- [File, tema chiaro](ui-ux-implemented/files-light.png)
- [Dettagli, tema scuro](ui-ux-implemented/details-dark.png)
- [Duplicati](ui-ux-implemented/duplicates-dark.png)
- [Preparazione](ui-ux-implemented/scan-form.png)
- [Scansione, 1100×700](ui-ux-implemented/scanning-1100.png)
- [Scansione, 800×500](ui-ux-implemented/scanning-800.png)
- [Risultato, 800×500](ui-ux-implemented/completed-800.png)

## Limiti rimasti

La paginazione limita gli elementi renderizzati; le API continuano a restituire la cartella o l'insieme dei gruppi duplicati in una risposta. La ricerca dei file è locale alla cartella. Activity conserva solo l'ultima scansione della sessione e non sopravvive alla chiusura dell'app. Pausa/ripresa persistente, cronologia su disco, ricerca globale e paginazione lato backend richiedono estensioni del motore e restano fuori da questa implementazione. L'arresto mantiene la semantica del backend: i contenuti già archiviati possono rimanere.

## Select Svelte personalizzate

Sostituite le cinque select native (archivio, aspetto, ordinamento file, ordinamento statistiche e azione delle regole) con `UiSelect.svelte`, basato su [Bits UI Select](https://bits-ui.com/docs/components/select), versione 2.19.2. Il menu usa i colori dell'app, indica la selezione e si posiziona rispetto allo spazio disponibile; supporta tastiera, Escape e chiusura esterna. Il wrapper esplicita il ruolo combobox e il collegamento al listbox per rendere valido `aria-activedescendant` nel menu aperto. Le etichette dei campi rispettano gli identificatori del componente. Migliorato anche il contrasto delle descrizioni dei preset, emerso dal controllo del modulo avanzato.

La suite aggiornata comprende 11 test UI superati, inclusi navigazione con frecce/Invio/Esc, ritorno del focus, stato disabilitato, accessibilità del menu aperto e selezione dell'azione tramite la sua etichetta. La fixture attende il primo rendering dopo il caricamento dei dati, evitando interazioni sui controlli SSR prima dell'idratazione.

[Menu archivio chiaro](ui-ux-implemented/select-light.png) · [Menu aspetto scuro a 800×500](ui-ux-implemented/select-dark.png).
