import { useQuery } from "@tanstack/react-query";
import { ArrowLeft } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, Navigate, useNavigate, useParams } from "react-router";
import { Card } from "@/components/ui/card";
import { Field } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { useLang } from "@/lib/use-language";
import { HelpPage } from "./HelpPage";
import { helpTitle, loadHelpPage } from "./pages";
import { HELP_GROUPS, HELP_TOPICS, isHelpTopic } from "./topics";

/** Accents, Arabic short vowels and hamza seats do not count when searching a title. */
const fold = (text: string) => text.normalize("NFD").replace(/\p{M}/gu, "").toLocaleLowerCase();

/** `/help`: every page, grouped, searchable by its title in the UI language. */
export function HelpCentre() {
  const { t } = useTranslation();
  const lang = useLang();
  const [search, setSearch] = useState("");
  // the 30-odd small pages of the language, once: their first line is the title
  const titles = useQuery({
    queryKey: ["help-titles", lang],
    queryFn: async () => {
      const texts = await Promise.all(HELP_TOPICS.map(({ id }) => loadHelpPage(lang, id)));
      return new Map(HELP_TOPICS.map(({ id }, i) => [id, helpTitle(texts[i] ?? null) ?? id]));
    },
    staleTime: Infinity,
  });
  const wanted = fold(search.trim());
  const matches = HELP_TOPICS.flatMap(({ id, group }) => {
    const title = titles.data?.get(id);
    return title !== undefined && fold(title).includes(wanted) ? [{ id, group, title }] : [];
  });
  return (
    <div className="flex max-w-5xl flex-col gap-6">
      <h1 className="text-3xl font-bold">{t("help:title")}</h1>
      <div className="max-w-xl">
        <Field id="help-search" label={t("help:search")}>
          <Input
            id="help-search"
            type="search"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
          />
        </Field>
      </div>
      {titles.isSuccess && matches.length === 0 ? <p role="status">{t("help:noResult")}</p> : null}
      <div className="grid gap-4 md:grid-cols-2">
        {HELP_GROUPS.map((group) => {
          const topics = matches.filter((topic) => topic.group === group);
          if (topics.length === 0) return null;
          return (
            <Card key={group} aria-labelledby={`help-group-${group}`}>
              <h2 id={`help-group-${group}`} className="text-xl font-bold">
                {t(`help:groups.${group}`)}
              </h2>
              <ul className="flex flex-col">
                {topics.map((topic) => (
                  <li key={topic.id}>
                    <Link
                      to={`/help/${topic.id}`}
                      className="flex min-h-11 items-center rounded-control px-2 font-semibold text-info hover:bg-surface-2"
                    >
                      {topic.title}
                    </Link>
                  </li>
                ))}
              </ul>
            </Card>
          );
        })}
      </div>
    </div>
  );
}

/** `/help/:topic`: the page, a way back to the centre; a `help:` link opens its page here. */
export function HelpRoute() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { topic = "" } = useParams();
  if (!isHelpTopic(topic)) return <Navigate to="/help" replace />;
  return (
    <div className="flex flex-col gap-4">
      <Link
        to="/help"
        className="inline-flex min-h-11 items-center gap-2 self-start rounded-control px-2 font-semibold text-info hover:bg-surface-2"
      >
        <ArrowLeft aria-hidden className="size-5 rtl:-scale-x-100" />
        {t("help:back")}
      </Link>
      <HelpPage topic={topic} onTopic={(next) => void navigate(`/help/${next}`)} />
    </div>
  );
}
