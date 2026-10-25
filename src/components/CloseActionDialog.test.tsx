import { renderToStaticMarkup } from "react-dom/server";
import { createInstance } from "i18next";
import { I18nextProvider } from "react-i18next";
import { expect, it } from "vitest";
import en from "../i18n/en.json";
import zh from "../i18n/zh.json";
import zhTW from "../i18n/zh-TW.json";
import es from "../i18n/es.json";
import ko from "../i18n/ko.json";
import { CloseActionDialog } from "./CloseActionDialog";

it.each(Object.entries({ en, zh, "zh-TW": zhTW, es, ko }))(
  "renders all close-action labels in %s without fallback",
  async (lng, translation) => {
    const i18n = createInstance();
    await i18n.init({ lng, fallbackLng: false, resources: { [lng]: { translation } } });
    const html = renderToStaticMarkup(
      <I18nextProvider i18n={i18n}>
        <CloseActionDialog open onCancel={() => {}} onClose={() => {}} onHide={() => {}} />
      </I18nextProvider>,
    );
    for (const key of ["title", "message", "remember", "close", "hide"]) {
      expect(i18n.exists(`closeAction.${key}`)).toBe(true);
      expect(html).toContain(i18n.t(`closeAction.${key}`));
    }
    expect(html).not.toContain("closeAction.");
  },
);
