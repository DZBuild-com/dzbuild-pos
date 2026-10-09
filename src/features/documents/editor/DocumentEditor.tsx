import { Eye, Save, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Navigate, useNavigate, useParams } from "react-router";
import { toast } from "sonner";
import { useStore } from "zustand";
import { Amount } from "@/components/amount";
import { LanguageToggle } from "@/components/language-toggle";
import { useApproval } from "@/components/manager-approval";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { NativeSelect } from "@/components/ui/native-select";
import { isApprovalCancelled } from "@/lib/approval";
import { useCtrlHotkey, useHotkeys } from "@/lib/hotkeys";
import { type Lang, LANGS, isLang } from "@/lib/i18n";
import { useCancelDraft, useDraftCart, useDraftPreview, useMentions, useSaveDraft } from "@/lib/ipc/billing";
import { useSetting } from "@/lib/ipc/settings";
import type { Document, DocType, DraftCart, DraftPreview, Session, ValidatedDocument } from "@/lib/ipc/types";
import { formatRate } from "@/lib/numeric";
import { useDebounced } from "@/lib/use-debounced";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";
import { mayOutput, newKinds } from "../kinds";
import { PreviewDialog } from "../PreviewDialog";
import { PrintDialog } from "../PrintDialog";
import { CustomerField } from "./CustomerField";
import { createEditorStore, defaultSecondLang, toInput } from "./editor-store";
import { EditorLines } from "./EditorLines";
import { MentionsPanel, blocksOnMentions, isComplete } from "./MentionsPanel";
import { type DraftRef, ValidateDialog } from "./ValidateDialog";

/** The kinds typed in the editor (a credit note is made from its invoice). */
export const EDITOR_KINDS: readonly DocType[] = ["invoice", "quote", "proforma", "delivery_note"];

/** `/documents/new/:kind`: a new document of a kind the user may make, else the archive. */
export function NewDocumentRoute({ session }: { session: Session }) {
  const { kind = "" } = useParams();
  const allowed = newKinds(session.permissions).find((k) => k === kind);
  if (!allowed) return <Navigate to="/documents" replace />;
  return <DocumentEditor key={allowed} kind={allowed} draftId={null} session={session} />;
}

/**
 * DOC-001: an invoice, a quote, a proforma or a delivery note typed at the desk — a new one
 * (`draftId` null) or a saved draft. It opens once the draft is read.
 */
export function DocumentEditor({
  kind,
  draftId,
  session,
}: {
  kind: DocType;
  draftId: string | null;
  session: Session;
}) {
  const { t } = useTranslation();
  const errorText = useErrorText();
  const draft = useDraftCart(draftId);
  // A new document starts in the second language the settings ask for (DOC-034).
  const layout = useSetting("documents.layout", draftId === null);
  if (draftId === null && layout.isPending) {
    return <p className="text-ink-2">{t("common:status.loading")}</p>;
  }
  if (draftId !== null && !draft.data) {
    return draft.error ? (
      <p role="alert" className="text-danger">
        {errorText(draft.error)}
      </p>
    ) : (
      <p className="text-ink-2">{t("common:status.loading")}</p>
    );
  }
  return (
    <Editor
      kind={draft.data?.docType ?? kind}
      initial={draft.data ?? null}
      layout={draftId === null ? layout.data : undefined}
      session={session}
    />
  );
}

/** The store of a new document (bilingual when `layout` says so), or of the draft as it was read. */
function initialStore(kind: DocType, lang: Lang, initial: DraftCart | null, layout: unknown) {
  const store = createEditorStore(kind, lang, defaultSecondLang(layout, lang));
  if (initial) store.getState().loadDraft(initial, {});
  return store;
}

function Editor({
  kind,
  initial,
  layout,
  session,
}: {
  kind: DocType;
  initial: DraftCart | null;
  layout: unknown;
  session: Session;
}) {
  const { t } = useTranslation();
  const uiLang = useLang();
  const errorText = useErrorText();
  const navigate = useNavigate();
  const [store] = useState(() => initialStore(kind, uiLang, initial, layout));
  const state = useStore(store);
  const input = useMemo(() => toInput(state), [state]);
  const current = useMemo(() => JSON.stringify(input), [input]);
  // What the backend holds: a change since makes the draft "dirty" (saved again before it is
  // validated or previewed).
  const [savedId, setSavedId] = useState<string | null>(initial?.id ?? null);
  const [saved, setSaved] = useState<string | null>(() => (initial ? current : null));
  const dirty = saved !== current;
  const typed = useDebounced(input, 200);
  // The saved draft goes with it: one converted from a quote keeps its offered prices (ORD-001).
  const previewInput = useMemo(
    () => (typed.cart.lines.length > 0 ? { ...typed, draftId: savedId } : null),
    [typed, savedId],
  );
  const preview = useDraftPreview(previewInput);
  const mentions = useMentions(state.customerId);
  const validity = useSetting("documents.quote_validity_days", kind === "quote" || kind === "proforma");
  const save = useSaveDraft();
  const cancel = useCancelDraft();
  const approval = useApproval();
  const [validating, setValidating] = useState<DraftRef | null>(null);
  const [previewing, setPreviewing] = useState<string | null>(null);
  const [validated, setValidated] = useState<Document | null>(null);
  const [confirmCancel, setConfirmCancel] = useState(false);
  const [busy, setBusy] = useState(false);

  const check = mentions.data ?? preview.data?.mentions;
  const blocked = blocksOnMentions(state.docType) && check !== undefined && !isComplete(check);
  const editable = validated === null && !busy;
  const canPreview = mayOutput(state.docType, session.permissions);

  /** Saves the draft as typed (a manager's approval first when a price or a discount needs one);
   * `stay` keeps this screen (validation, preview), else a new draft moves to its own address. */
  const persist = async (stay: boolean): Promise<string | null> => {
    const sent = toInput(store.getState());
    setBusy(true);
    try {
      const doc = await approval.run(() => save.mutateAsync({ id: savedId, input: sent }));
      setSavedId(doc.id);
      setSaved(JSON.stringify(sent));
      if (!stay) {
        toast.success(t("billing:editor.saved"));
        if (savedId === null) void navigate(`/documents/${doc.id}`, { replace: true });
      }
      return doc.id;
    } catch (error) {
      if (!isApprovalCancelled(error)) toast.error(errorText(error));
      return null;
    } finally {
      setBusy(false);
    }
  };
  /** The saved draft, saved again first when it changed since. */
  const savedDraft = async (): Promise<string | null> =>
    savedId !== null && !dirty ? savedId : persist(true);

  /** The saved draft as the validation dialog reads it (its languages and products for the Arabic
   * guard). */
  const draftRef = (id: string): DraftRef => {
    const { docType, customerId, lang, secondLang, lines } = store.getState();
    const productIds = [...new Set(lines.flatMap((l) => (l.line.productId ? [l.line.productId] : [])))];
    return { id, docType, customerId, lang, secondLang, productIds };
  };
  const startValidation = async () => {
    const id = await savedDraft();
    if (id) setValidating(draftRef(id));
  };
  /** DOC-034: the Arabic guard's suggestion — Arabic first, the language the draft had second (AR +
   * FR) unless it already has one — saved at once, so the validation that follows numbers it. */
  const switchLang = async (next: Lang) => {
    const previous = store.getState().lang;
    store.getState().setLang(next);
    if (store.getState().secondLang === null && previous !== next) store.getState().setSecondLang(previous);
    const id = await persist(true);
    if (id) setValidating(draftRef(id));
  };
  const startPreview = async () => {
    const id = await savedDraft();
    if (id) setPreviewing(id);
  };
  const discard = () => {
    if (savedId === null) return;
    cancel.mutate(savedId, {
      onSuccess: () => {
        toast.success(t("billing:editor.cancelled"));
        void navigate("/documents", { replace: true });
      },
      onError: (error) => toast.error(errorText(error)),
    });
  };

  useCtrlHotkey("s", () => void persist(false), editable);
  useHotkeys({ F10: () => void startValidation() }, editable && !blocked);

  const title =
    savedId === null
      ? t(`billing:editor.new.${state.docType}`)
      : t("billing:editor.draftOf", { kind: t(`billing:kinds.${state.docType}`) });
  const offer = state.docType === "quote" || state.docType === "proforma";
  const days = typeof validity.data === "number" ? validity.data : null;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <h1 className="text-3xl font-bold">{title}</h1>
        <div className="flex flex-wrap gap-2">
          <Button
            variant="secondary"
            disabled={!editable}
            onClick={() => void persist(false)}
            shortcut="Control+S"
            shortcutLabel={t("common:keys.ctrlS")}
          >
            <Save aria-hidden />
            {t("billing:editor.save")}
          </Button>
          <Button
            variant="outline"
            disabled={!editable || savedId === null || !canPreview}
            onClick={() => void startPreview()}
          >
            <Eye aria-hidden />
            {t("billing:editor.preview")}
          </Button>
          <Button
            variant="primary"
            disabled={!editable || blocked}
            onClick={() => void startValidation()}
            shortcut="F10"
            shortcutLabel={t("common:keys.f10")}
          >
            {t("billing:editor.validate")}
          </Button>
        </div>
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        <CustomerField
          value={state.customerId}
          onChange={(id) => store.getState().setCustomer(id)}
          disabled={!editable}
        />
        <div className="flex flex-col gap-1.5">
          <span className="font-semibold">{t("billing:editor.language")}</span>
          <LanguageToggle value={state.lang} onChange={(l) => store.getState().setLang(l)} />
        </div>
        <Field id="editor-second" label={t("billing:editor.secondLanguage")}>
          <NativeSelect
            id="editor-second"
            value={state.secondLang ?? ""}
            disabled={!editable}
            onChange={(event) => {
              const value = event.target.value;
              store.getState().setSecondLang(isLang(value) ? value : null);
            }}
          >
            <option value="">{t("billing:editor.none")}</option>
            {LANGS.filter((l) => l !== state.lang).map((l) => (
              <option key={l} value={l} lang={l}>
                {t(`common:languages.${l}`)}
              </option>
            ))}
          </NativeSelect>
        </Field>
        {offer ? (
          <Field
            id="editor-valid-until"
            label={t("billing:editor.validUntil")}
            hint={days !== null ? t("billing:editor.validUntilDefault", { days }) : undefined}
          >
            <Input
              id="editor-valid-until"
              type="date"
              dir="ltr"
              disabled={!editable}
              value={state.validUntil ?? ""}
              onChange={(event) => store.getState().setValidUntil(event.target.value || null)}
            />
          </Field>
        ) : null}
        {state.docType === "invoice" ? (
          <Field id="editor-due" label={t("billing:editor.dueDate")}>
            <Input
              id="editor-due"
              type="date"
              dir="ltr"
              disabled={!editable}
              value={state.dueDate ?? ""}
              onChange={(event) => store.getState().setDueDate(event.target.value || null)}
            />
          </Field>
        ) : null}
        <Field id="editor-note" label={t("billing:editor.note")}>
          <Input
            id="editor-note"
            dir="auto"
            maxLength={200}
            disabled={!editable}
            value={state.note ?? ""}
            onChange={(event) =>
              store.getState().setNote(event.target.value === "" ? null : event.target.value)
            }
          />
        </Field>
      </div>

      {check ? <MentionsPanel check={check} docType={state.docType} customerId={state.customerId} /> : null}

      <EditorLines
        store={store}
        priced={state.lines.length > 0 ? preview.data?.priced.cart.lines : undefined}
        ht={preview.data?.priced.cart.priceMode === "ht"}
      />
      {preview.error ? (
        <p role="alert" className="text-danger">
          {errorText(preview.error)}
        </p>
      ) : null}
      {state.lines.length > 0 && preview.data ? <Totals preview={preview.data} /> : null}

      {savedId !== null && validated === null ? (
        <div className="flex flex-wrap items-center gap-3 border-t border-line pt-4">
          {confirmCancel ? (
            <>
              <p className="text-danger">{t("billing:editor.cancelConfirm")}</p>
              <Button variant="danger" disabled={cancel.isPending} onClick={discard}>
                {t("common:actions.confirm")}
              </Button>
              <Button variant="ghost" onClick={() => setConfirmCancel(false)}>
                {t("common:actions.cancel")}
              </Button>
            </>
          ) : (
            <Button variant="danger" disabled={!editable} onClick={() => setConfirmCancel(true)}>
              <Trash2 aria-hidden />
              {t("billing:editor.cancelDraft")}
            </Button>
          )}
        </div>
      ) : null}

      {validating ? (
        <ValidateDialog
          draft={validating}
          onClose={() => setValidating(null)}
          onSwitchLang={(next) => void switchLang(next)}
          onValidated={(done: ValidatedDocument) => {
            setValidating(null);
            setValidated(done.document);
          }}
        />
      ) : null}
      {validated ? (
        <PrintDialog
          id={validated.id}
          docType={validated.docType}
          doc={validated}
          open
          onClose={() => void navigate(`/documents/${validated.id}`, { replace: true })}
        />
      ) : null}
      {previewing ? (
        <PreviewDialog id={previewing} options={{}} open onClose={() => setPreviewing(null)} />
      ) : null}
      {approval.dialog}
    </div>
  );
}

/** The backend's totals: per rate HT and TVA then TTC; an IFU document shows TTC only with its
 * mention (CTCA art. 64); an exempt buyer is invoiced HT without TVA (TAX-004). */
function Totals({ preview }: { preview: DraftPreview }) {
  const { t } = useTranslation();
  const lang = useLang();
  const cart = preview.priced.cart;
  const totals = cart.totals;
  const ifu = cart.taxRegime === "ifu";
  const exempt = preview.priced.vatExempt;
  const rows: { label: string; value: number; strong?: boolean }[] = [];
  if (!ifu) {
    rows.push({ label: t("billing:editor.totals.ht"), value: totals.subtotalHt });
    if (!exempt) {
      for (const rate of totals.perRate) {
        rows.push({
          label: t("billing:editor.totals.tax", { rate: formatRate(rate.rateBp, lang) }),
          value: rate.tax,
        });
      }
    }
  }
  rows.push({ label: t("billing:editor.totals.ttc"), value: totals.totalTtc, strong: true });
  return (
    <section aria-labelledby="editor-totals" className="flex flex-col gap-2 self-end">
      <h2 id="editor-totals" className="sr-only">
        {t("billing:view.totals")}
      </h2>
      {ifu ? <p className="text-ink-2">{t("billing:editor.totals.ifu")}</p> : null}
      {exempt ? (
        <div>
          <Badge tone="info">{t("billing:editor.totals.exempt")}</Badge>
        </div>
      ) : null}
      <dl className="grid min-w-72 grid-cols-[1fr_auto] gap-x-6 gap-y-1">
        {rows.map(({ label, value, strong }) => (
          <div key={label} className={strong ? "contents text-lg font-bold" : "contents"}>
            <dt>{label}</dt>
            <dd className="text-end">
              <Amount value={value} />
            </dd>
          </div>
        ))}
      </dl>
    </section>
  );
}
