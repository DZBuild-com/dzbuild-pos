// The French texts the till needs (every namespace but the back office's), one chunk read by
// `loadLanguage` (src/lib/i18n.ts) before the first paint in French: it ships inside the app, so
// this works offline. The back-office namespaces are in `fr-office.ts` (NFR-010, finding 28).
export default import.meta.glob<Record<string, unknown>>(
  [
    "../fr/*.json",
    "!../fr/{products,promotions,stock,documents,purchases,reports,imports,tables,fiscal,packs,restaurant,pharmacy,butcher,phones,services,instalments,orders,wholesale,loyalty,jewellery,staff,zakat,privacy,config,settings,license}.json",
  ],
  { eager: true, import: "default" },
);
