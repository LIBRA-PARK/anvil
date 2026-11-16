// Monaco 설정. 번들이 크므로 이 모듈은 에디터·Diff 탭을 처음 열 때만(동적 import) 불러온다.
import * as monaco from "monaco-editor";
import EditorWorker from "../../node_modules/monaco-editor/esm/vs/editor/editor.worker.js?worker";
import CssWorker from "../../node_modules/monaco-editor/esm/vs/language/css/css.worker.js?worker";
import HtmlWorker from "../../node_modules/monaco-editor/esm/vs/language/html/html.worker.js?worker";
import JsonWorker from "../../node_modules/monaco-editor/esm/vs/language/json/json.worker.js?worker";
import TsWorker from "../../node_modules/monaco-editor/esm/vs/language/typescript/ts.worker.js?worker";

// (패키지의 exports 패턴을 Vite가 워커 경로에서 해석하지 못해 파일 경로로 직접 가져온다)
// 데스크톱 앱이므로 CDN이 아니라 번들에 포함된 워커를 쓴다.
self.MonacoEnvironment = {
  getWorker(_workerId, label) {
    switch (label) {
      case "json":
        return new JsonWorker();
      case "css":
      case "scss":
      case "less":
        return new CssWorker();
      case "html":
      case "handlebars":
      case "razor":
        return new HtmlWorker();
      case "typescript":
      case "javascript":
        return new TsWorker();
      default:
        return new EditorWorker();
    }
  },
};

monaco.editor.defineTheme("anvil", {
  base: "vs-dark",
  inherit: true,
  rules: [],
  colors: {
    "editor.background": "#1f2023",
    "editorGutter.background": "#1f2023",
    "editorCursor.foreground": "#e0763c",
  },
});
monaco.editor.setTheme("anvil");

/** 에디터와 Diff 뷰어가 함께 쓰는 기본 옵션 */
export const BASE_OPTIONS: monaco.editor.IEditorOptions = {
  automaticLayout: true,
  fontFamily: 'Menlo, Consolas, "DejaVu Sans Mono", "D2Coding", monospace',
  fontSize: 13,
  scrollBeyondLastLine: false,
  minimap: { enabled: false },
};

/**
 * 파일 하나를 가리키는 Monaco URI. 확장자로 언어가 정해진다.
 * 같은 경로라도 호스트나 용도(`tag`)가 다르면 다른 모델이 되도록 구분한다.
 */
export function fileUri(hostId: string, path: string, tag = ""): monaco.Uri {
  const normalized = path.replace(/\\/g, "/");
  return monaco.Uri.from({
    scheme: "anvil",
    authority: hostId,
    path: normalized.startsWith("/") ? normalized : `/${normalized}`,
    query: tag,
  });
}

/** 같은 URI의 모델이 남아 있으면 정리하고 새로 만든다. */
export function createModel(text: string, uri: monaco.Uri): monaco.editor.ITextModel {
  monaco.editor.getModel(uri)?.dispose();
  return monaco.editor.createModel(text, undefined, uri);
}

export { monaco };
