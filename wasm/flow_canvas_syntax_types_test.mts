import type { FlowCanvasColor, FlowCanvasOptions } from "./flow-canvas.js";

const roles: FlowCanvasColor[] = [
  "tok-kw", "tok-ty", "tok-fn", "tok-st", "tok-nu", "tok-cm", "tok-op", "tok-pn",
];
const options: FlowCanvasOptions = {
  colors: {
    "tok-kw": "red", "tok-ty": "orange", "tok-fn": "purple", "tok-st": "navy",
    "tok-nu": "blue", "tok-cm": "gray", "tok-op": "black", "tok-pn": "black",
  },
};
void roles;
void options;
// @ts-expect-error Unknown lexer classes remain outside the palette contract.
const badRole: FlowCanvasColor = "tok-unknown";
// @ts-expect-error Palette values must remain CSS color strings.
const badColor: FlowCanvasOptions = { colors: { "tok-kw": 42 } };
void badRole;
void badColor;
