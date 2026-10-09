import { ImageOff, ImagePlus, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { useAppDataDir } from "@/lib/ipc/app";
import { useClearImage, usePickImage } from "@/lib/ipc/product-extras";
import type { Product } from "@/lib/ipc/types";
import { mediaUrl } from "@/lib/media";
import { localizedName } from "@/lib/names";
import { useErrorText } from "@/lib/use-error-text";
import { useLang } from "@/lib/use-language";

/**
 * PRD-004: the product's photo, as its thumbnail under `<data>/media`. "Choisir une photo" opens
 * the Rust file dialog (the webview never names a path); the backend resizes the picture and keeps
 * the previous one until the new one is written.
 */
export function ProductPhoto({ product, editable }: { product: Product; editable: boolean }) {
  const { t } = useTranslation();
  const lang = useLang();
  const errorText = useErrorText();
  const dataDir = useAppDataDir().data;
  const pick = usePickImage();
  const clear = useClearImage();
  const src = dataDir ? mediaUrl(dataDir, product.image, true) : null;
  const error = pick.error ?? clear.error;
  const busy = pick.isPending || clear.isPending;
  return (
    <section className="flex flex-wrap items-center gap-4">
      {src ? (
        <img
          src={src}
          alt={t("products:photo.alt", { name: localizedName(product, lang) })}
          className="size-32 rounded-card border border-line object-cover"
        />
      ) : (
        <div className="grid size-32 place-items-center rounded-card border border-dashed border-line bg-surface-2 text-center text-sm text-ink-2">
          <span className="flex flex-col items-center gap-1">
            <ImageOff aria-hidden className="size-6" />
            {t("products:photo.none")}
          </span>
        </div>
      )}
      {editable ? (
        <div className="flex flex-col gap-2">
          <div className="flex flex-wrap gap-2">
            <Button variant="outline" disabled={busy} onClick={() => pick.mutate(product.id)}>
              <ImagePlus aria-hidden />
              {t("products:photo.pick")}
            </Button>
            {product.image ? (
              <Button variant="ghost" disabled={busy} onClick={() => clear.mutate(product.id)}>
                <Trash2 aria-hidden />
                {t("products:photo.remove")}
              </Button>
            ) : null}
          </div>
          <p className="text-sm text-ink-2">{t("products:photo.hint")}</p>
        </div>
      ) : null}
      {error ? (
        <p role="alert" className="w-full text-danger">
          {errorText(error)}
        </p>
      ) : null}
    </section>
  );
}
