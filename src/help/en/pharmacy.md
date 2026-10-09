# Pharmacy: medicines, expiry, Chifa list and register

## The medicine details

Open a product in **Products**, tab **Medicine** (the Pharmacy module must be on).

- **INN**: the generic name. The medicines with the same INN are listed below, the cheapest first: the till offers them as generics.
- **Controlled medicine**: dispensing it needs the prescription and goes into the register. The **Schedule** (A, B or C) takes 20 characters at most.
- **Prescription only**: the till asks for the prescription.
- The Chifa reference price shows when the imported list knows the product. It is for information.

## Expiry

In **Stock**, tab **Expiry**, the board first shows the expired lots, then the ones expiring within 7, 30, 60 or 90 days, with their value at cost. Export or print them with the Excel, CSV and PDF buttons. Under the board, the lot list removes an expired lot from the stock.

## The Chifa list

In **Products**, tab **Chifa list**:

1. Choose the file you received (Excel, OpenDocument or CSV).
2. Say which column holds the code, the name, the reference price and, when the file has one, the "reimbursable" column.
3. Tap **Check**: every row to fix is listed with its column.
4. Once the file has no error, tap **Apply**: the whole list is applied at once.

## The controlled medicines register

In **Products**, tab **Register**, reserved to the users holding the register's permission. Choose the period: every dispensing is there with its number, its ticket, the prescriber, the prescription and the patient. **Print the register** saves the paged register as a PDF.

> This is health data: show it only to the people allowed to see it. The register is kept on the main PC: a till writes each dispensing there as soon as the sale is made, and sells a controlled medicine only while connected to the main PC. On a till, the register is read from the main PC and printed there.

- [ ] Fill in the INN and the controlled status of the medicines
- [ ] Import the Chifa list
- [ ] Check the expiry every week
