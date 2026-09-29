# Guida rapida — Numerazione punti

> **In prova:** prima di operare su disegni importanti, salva una copia del DWG/DXF e verifica le etichette create.

## Cosa fa

Il plugin inserisce etichette di testo numeriche nel disegno. Dopo l’avvio, fai clic su un vertice o in un punto vuoto: viene inserita un’etichetta leggermente in alto a destra del clic. Il valore successivo viene mantenuto finché OpenCADStudio resta aperto.

## Prima installazione su Windows

1. Apri la [release più recente](https://github.com/franzo-ux/opencad-xftools/releases/latest).
2. Scarica **entrambi** i file:
   - `opencad.point_numbering-windows-x86_64.dll`
   - `plugin.toml`
3. In Esplora file, incolla questo percorso nella barra degli indirizzi:

   ```text
   %APPDATA%\OpenCADStudio\plugins\opencad.point_numbering\
   ```

4. Se la cartella non esiste, creala.
5. Copia entrambi i file al suo interno.
6. Riavvia OpenCADStudio.

Dopo il riavvio compare la scheda **xfTools** nel ribbon, con il pulsante **Number points** e l’icona `1,2…`.

## Installazione su macOS (Apple Silicon)

1. Apri la [release più recente](https://github.com/franzo-ux/opencad-xftools/releases/latest).
2. Scarica `opencad.point_numbering-macos-aarch64.dylib` e `plugin.toml`.
3. Copia entrambi in:

   ```text
   ~/Library/Application Support/OpenCADStudio/plugins/opencad.point_numbering/
   ```

4. Riavvia OpenCADStudio.

## Installazione su Linux (x86_64)

1. Apri la [release più recente](https://github.com/franzo-ux/opencad-xftools/releases/latest).
2. Scarica `opencad.point_numbering-linux-x86_64.so` e `plugin.toml`.
3. Copia entrambi in:

   ```text
   ~/.config/OpenCADStudio/plugins/opencad.point_numbering/
   ```

4. Riavvia OpenCADStudio.

## Numerazione standard

1. Apri un disegno.
2. Seleziona **xfTools → Number points** oppure digita `PNUM` nella riga di comando.
3. Fai clic sui punti nell’ordine desiderato.
4. Premi **Invio** o **Esc** per terminare.

La prima esecuzione parte da `1`, aumenta di `1` e non ha prefisso.

## Punti di riferimento da PDF

1. Allega prima il PDF con il comando nativo di OpenCADStudio.
2. Seleziona **xfTools → PDF reference points** oppure digita `PDFREFS`.
3. xfTools legge le linee vettoriali blu del PDF e crea punti CAD sui loro vertici.

I punti creati sono riferimenti agganciabili con gli snap CAD e servono, ad esempio, per scalare il foglio in base alle tacche blu. Le parti rasterizzate del PDF non sono rilevabili.

## Racchiudere testi esistenti

1. Seleziona **xfTools → Enclose text** oppure digita `TFRAME`.
2. Scegli rettangolo, cerchio o slot; scegli **Fit** per adattare la forma al testo più l’offset, oppure **Fixed** per inserire dimensioni fisse.
3. Seleziona i testi `TEXT` da racchiudere; premi **Invio** o **Esc** per terminare.

Su Linux usa il formato `TFRAME:rectangle,fit,1,10,5` (forma, modalità, offset, larghezza, altezza/diametro).

## Configurare o riavviare la numerazione

Ogni avvio di `PNUM` apre una finestra con questi campi:

- numero iniziale;
- incremento (può essere negativo);
- prefisso;
- altezza del testo;
- offset alto-destra;
- stile testo.

Modifica i valori e scegli **Start**. Scegli **Cancel** per non avviare la numerazione. I valori dell’ultimo avvio restano proposti fino alla chiusura di OpenCADStudio.

## Aspetto delle etichette

- Le etichette sono entità `TEXT`, quindi puoi selezionarle, spostarle, modificarle o eliminarle come normale testo CAD.
- Altezza, offset e stile derivano dai valori scelti nella finestra.

## Risoluzione problemi

| Problema | Cosa verificare |
| --- | --- |
| La scheda **xfTools** non compare | Controlla che DLL e `plugin.toml` siano nella stessa cartella e riavvia OpenCADStudio. |
| Il plugin non viene caricato | Verifica di usare OpenCADStudio `v2026.38`; il plugin dipende dalla sua API. |
| Il testo è troppo piccolo/grande | Imposta l’altezza desiderata nella finestra prima di scegliere **Start**. |
| Il numero non riparte | Imposta il numero iniziale desiderato nella finestra prima di scegliere **Start**. |
