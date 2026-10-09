# Hardware

Hardware is set up for each PC, tab by tab. Nothing is kept before "Save the hardware settings".

## Receipt printer

- In the "Receipt printers" tab, "Detect printers" offers the ones Windows knows.
- "Print a test page" checks the cable and the paper.
- Choose the paper width: 58 or 80 mm.
- Arabic prints as an image ("Image (Arabic)"): it comes out right on every receipt printer.
- "Check the printers" points out a printer that is unplugged, offline or whose driver changed.
- A Windows printer that refuses direct printing is set to "Windows driver (pages)" with its roll width (58 or 80 mm): the ticket then prints as a page.

## Drawer, scale, display

- The cash drawer plugs into the receipt printer; the "Drawer" tab tests it with "Open the drawer".
- The scale plugs into a COM (serial) port; at the till, `F11` weighs the item.
- The "Customer displays" tab sets up the pole display (VFD) and the customer screen, which show the lines and the total to the customer.

## Barcode scanner

- In the "Scanners" tab, "Learn my scanner": scan the barcode of the test page.
- The scanner is then recognised, set to AZERTY or QWERTY, and codes are read right even when Windows uses the Arabic keyboard.

## Kitchen printers

- In a restaurant, the "Kitchen" tab chooses for each preparation station (kitchen, grill, bar…) the printer of its tickets, the number of copies (1 to 3) and the buzzer.
- A station without a chosen printer prints on the fallback printer, else on the receipt printer.
- Each till sets this for itself: a network kitchen printer serves them all.

## Label printer

- The "Labels" tab adds the label printer (TSPL or ZPL: the printer's language, written in its manual); "Print a test label" checks it.
- Templates and label printing are in [Products › Labels](help:labels).
