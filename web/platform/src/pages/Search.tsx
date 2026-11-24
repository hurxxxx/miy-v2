import { useEffect, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Link, useLocation, useNavigate } from "@tanstack/react-router";
import { request, type SearchPage } from "@miy/sdk";
import { useInstallations } from "../data/installations";
import { useSurface } from "../data/session";
import { appLink, environmentPath, portalEnvironment } from "../lib/paths";
import { formatDateTime } from "../platform/time/time-utils";
import { Badge, Button, EmptyState, Loading, Notice, Panel } from "../ui";

type State =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "ready"; page: SearchPage }
  | { status: "error" };

/**
 * Integrated search (docs/COMMON-FEATURES-PLAN.md M12): each app the person may
 * use answers for them; this page shows what came back, grouped by app, as
 * text. An app that did not answer in time is marked, not hidden.
 */
export function Search() {
  const { t, i18n } = useTranslation("portal");
  const surface = useSurface();
  const navigate = useNavigate();
  const location = useLocation();
  const environment = portalEnvironment(surface.surface, surface.legacyPortal) ?? "dev";
  const { items: installations } = useInstallations();
  const { q } = location.search;
  const query = typeof q === "string" ? q.trim() : "";
  const [draft, setDraft] = useState(query);
  const [state, setState] = useState<State>({ status: "idle" });
  const [attempt, setAttempt] = useState(0);
  useEffect(() => setDraft(query), [query]);
  useEffect(() => {
    if (!query) {
      setState({ status: "idle" });
      return;
    }
    const controller = new AbortController();
    setState({ status: "loading" });
    request<SearchPage>(`/api/v1/search?q=${encodeURIComponent(query)}&limit=10`, undefined, { signal: controller.signal })
      .then((page) => {
        if (!controller.signal.aborted) setState({ status: "ready", page });
      })
      .catch(() => {
        if (!controller.signal.aborted) setState({ status: "error" });
      });
    return () => controller.abort();
  }, [query, attempt]);

  function submit(event: FormEvent) {
    event.preventDefault();
    const next = draft.trim();
    if (!next) return;
    if (next === query) setAttempt((value) => value + 1);
    else void navigate({ to: "/search", search: { q: next } as never });
  }

  const page = state.status === "ready" && state.page.query === query ? state.page : null;
  const visible = installations && new Set(installations.map((installation) => installation.id));
  const found = (page?.groups ?? []).filter((group) =>
    (!visible || visible.has(group.installationId)) && (group.status !== "ok" || group.items.length > 0),
  );
  return (
    <>
      <div className="page-header">
        <div>
          <h1>{t("search.title")}</h1>
          <p>{t("search.lead")}</p>
        </div>
      </div>
      <form className="search-form" role="search" onSubmit={submit}>
        <input
          className="app-field-input"
          type="search"
          aria-label={t("search.field")}
          placeholder={t("search.placeholder")}
          maxLength={200}
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
        />
        <Button type="submit" variant="primary" disabled={!draft.trim()}>
          {t("search.submit")}
        </Button>
      </form>
      {state.status === "loading" && <Loading>{t("search.loading")}</Loading>}
      {state.status === "error" && (
        <Notice
          role="alert"
          tone="danger"
          actions={
            <Button size="sm" onClick={() => setAttempt((value) => value + 1)}>
              {t("common:actions.retry")}
            </Button>
          }
        >
          <p>{t("search.failed")}</p>
        </Notice>
      )}
      {page && found.length === 0 && (
        <EmptyState title={t("search.emptyTitle", { query: page.query })}>{t("search.emptyHint")}</EmptyState>
      )}
      {found.map((group) => {
        const base = environmentPath(environment, group.appId);
        return (
          <Panel key={group.installationId} className="search-group" title={group.appName}>
            {group.status !== "ok" ? (
              <p className="home-card-note">{t("search.unavailable")}</p>
            ) : (
              <ul className="search-results">
                {group.items.map((item, index) => {
                  const destination = appLink(base, item.path);
                  const body = (
                    <>
                      <span className="search-result-title">
                        {item.kind && <Badge tone="neutral">{item.kind}</Badge>}
                        <span>{item.title}</span>
                      </span>
                      {(item.snippet || item.at) && (
                        <span className="search-result-meta">
                          {[item.snippet, item.at ? formatDateTime(item.at, { locale: i18n.language }) : null]
                            .filter(Boolean)
                            .join(" · ")}
                        </span>
                      )}
                    </>
                  );
                  return (
                    <li key={index}>
                      {destination ? (
                        <Link to={destination as "/"} className="search-result">
                          {body}
                        </Link>
                      ) : (
                        <span className="search-result">{body}</span>
                      )}
                    </li>
                  );
                })}
              </ul>
            )}
          </Panel>
        );
      })}
    </>
  );
}
