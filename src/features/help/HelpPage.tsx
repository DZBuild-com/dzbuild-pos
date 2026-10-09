import { useQuery } from "@tanstack/react-query";
import { Printer } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useInRouterContext, useNavigate } from "react-router";
import { Button } from "@/components/ui/button";
import { SupportCard } from "@/features/settings/SupportCard";
import { useLang } from "@/lib/use-language";
import { HelpMarkdown } from "./markdown";
import { loadHelpPage } from "./pages";

interface HelpPageProps {
  topic: string;
  /** Another help page was asked for (a `help:` link). */
  onTopic: (topic: string) => void;
}

/** One help page in the UI language, offline; its screen links work inside the app's router and
 * are plain words elsewhere (the setup wizard's tour). */
export function HelpPage(props: HelpPageProps) {
  return useInRouterContext() ? <RoutedHelpPage {...props} /> : <HelpPageBody {...props} />;
}

function RoutedHelpPage(props: HelpPageProps) {
  const navigate = useNavigate();
  return <HelpPageBody {...props} onRoute={(path) => void navigate(path)} />;
}

function HelpPageBody({ topic, onTopic, onRoute }: HelpPageProps & { onRoute?: (path: string) => void }) {
  const { t } = useTranslation();
  const lang = useLang();
  const page = useQuery({
    queryKey: ["help", lang, topic],
    queryFn: () => loadHelpPage(lang, topic),
    staleTime: Infinity,
  });
  if (page.isPending) return null;
  return (
    <div className="flex flex-col gap-6">
      <article className="print-sheet flex max-w-3xl flex-col gap-4">
        {page.data ? (
          <HelpMarkdown
            key={topic}
            text={page.data}
            onTopic={onTopic}
            onRoute={onRoute}
            checklistKey={topic}
          />
        ) : (
          <p role="alert">{t("help:noResult")}</p>
        )}
      </article>
      {topic === "quick-card" ? (
        <div>
          <Button variant="primary" onClick={() => window.print()}>
            <Printer aria-hidden />
            {t("help:print")}
          </Button>
        </div>
      ) : null}
      <div className="max-w-3xl">
        <SupportCard session={null} title={t("help:needHelp")} />
      </div>
    </div>
  );
}
