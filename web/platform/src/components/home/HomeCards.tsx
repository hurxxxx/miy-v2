import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "@tanstack/react-router";
import type { HomeCard } from "@miy/sdk";
import { useHomeCards } from "../../data/home";
import { formatDateTime } from "../../platform/time/time-utils";
import { Badge, Button, Panel } from "../../ui";

/**
 * The home's summary cards (docs/COMMON-FEATURES-PLAN.md M5): read-only lines
 * the apps return to the platform call, shown as text. Each line opens its app
 * at the app-relative path. Nothing shows when no app declares a card.
 */
export function HomeCards({ environmentPath }: { environmentPath: (appId: string) => string }) {
  const { t, i18n } = useTranslation("portal");
  const [attempt, setAttempt] = useState(0);
  const state = useHomeCards(true, attempt);
  if (state.status !== "error" && state.cards.length === 0) return null;
  return (
    <section aria-labelledby="home-cards" className="home-cards">
      <div className="home-cards-header">
        <h2 className="page-section-title" id="home-cards">
          {t("homeCards.title")}
        </h2>
        <Button size="sm" variant="ghost" disabled={state.status === "loading"} onClick={() => setAttempt((value) => value + 1)}>
          {t("homeCards.refresh")}
        </Button>
      </div>
      {state.status === "error" && state.cards.length === 0 && <p className="home-card-note">{t("homeCards.loadFailed")}</p>}
      <div className="card-grid">
        {state.cards.map((card) => (
          <SummaryCard key={card.installationId} card={card} base={environmentPath(card.appId)} locale={i18n.language} />
        ))}
      </div>
    </section>
  );
}

function SummaryCard({ card, base, locale }: { card: HomeCard; base: string; locale: string }) {
  const { t } = useTranslation("portal");
  const heading = card.title ?? card.appName;
  return (
    <Panel
      className="home-card"
      title={
        <span className="home-card-title">
          <span>{heading}</span>
          {card.status === "ok" && typeof card.count === "number" && <Badge tone="info">{card.count}</Badge>}
        </span>
      }
    >
      {card.title && <p className="home-card-app">{card.appName}</p>}
      {card.status !== "ok" ? (
        <p className="home-card-note">{t("homeCards.unavailable")}</p>
      ) : card.items.length === 0 ? (
        <p className="home-card-note">{t("homeCards.empty")}</p>
      ) : (
        <ul className="home-card-items">
          {card.items.map((item, index) => {
            const body = (
              <>
                <span className="home-card-item-title">{item.title}</span>
                {(item.subtitle || item.at) && (
                  <span className="home-card-item-meta">
                    {[item.subtitle, item.at ? formatDateTime(item.at, { locale }) : null].filter(Boolean).join(" · ")}
                  </span>
                )}
              </>
            );
            return (
              <li key={index}>
                {item.path ? (
                  <Link to={`${base}${item.path}` as "/"} className="home-card-item">
                    {body}
                  </Link>
                ) : (
                  <span className="home-card-item">{body}</span>
                )}
              </li>
            );
          })}
        </ul>
      )}
      <Link to={base as "/"} className="home-card-open">
        {t("home.open")}
      </Link>
    </Panel>
  );
}
