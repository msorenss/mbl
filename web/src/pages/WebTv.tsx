import { OnlineSearch } from "../components/OnlineSearch";
import { useAuth } from "../context/AuthContext";
import { useI18n } from "../i18n";

export function WebTv() {
  const { user } = useAuth();
  const { t } = useI18n();

  return (
    <div style={{ "max-width": "1100px", margin: "0 auto", padding: "16px 20px 48px" }}>
      {/* Page Header */}
      <div
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
          "margin-bottom": "24px",
          "border-bottom": "1px solid var(--border)",
          "padding-bottom": "16px",
        }}
      >
        <div>
          <div style={{ display: "flex", "align-items": "center", gap: "12px" }}>
            <span style={{ "font-size": "32px", "line-height": 1 }}>🌐</span>
            <h1
              style={{
                margin: 0,
                "font-size": "26px",
                "font-weight": "800",
                "letter-spacing": "-0.5px",
              }}
            >
              WebTV
            </h1>
          </div>
          <p style={{ margin: "6px 0 0", color: "var(--text-secondary)", "font-size": "14px" }}>
            {t("onlineDescription")}
          </p>
        </div>
      </div>

      {/* Embedded WebTV Content & Player */}
      <OnlineSearch admin={user()?.isAdmin} />
    </div>
  );
}

export default WebTv;
