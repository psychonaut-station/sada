import { css, html, LitElement, type TemplateResult } from "lit";
import { customElement } from "lit/decorators.js";

@customElement("controls-page")
export class ControlsPage extends LitElement {
    static styles = css`
        :host {
          display: block;
        }
    `;

    protected render(): TemplateResult {
        return html``;
    }
}
