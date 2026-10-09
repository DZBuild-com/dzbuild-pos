// The French texts of the back office only (`BACK_OFFICE_NAMESPACES` in src/lib/i18n.ts), one
// chunk read the first time a back-office screen opens in French (NFR-010, finding 28).
export default import.meta.glob<Record<string, unknown>>(
  "../fr/{products,promotions,stock,documents,purchases,reports,imports,tables,fiscal,packs,restaurant,pharmacy,butcher,phones,services,instalments,orders,wholesale,loyalty,jewellery,staff,zakat,privacy,config,settings,license}.json",
  { eager: true, import: "default" },
);
