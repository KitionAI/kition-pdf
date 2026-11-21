/**
 * Tessera icons for the PDF engine's commands that @kition/sdk/editor's rules cannot read (they draw
 * the neutral command mark otherwise): its own vocabulary, by command id. Registered once, when the
 * editor loads (`main.tsx`); ribbons and menus built from engine commands draw them.
 */
import { TesseraIcons, registerCommandIcons } from '@kition/sdk/editor'


registerCommandIcons({
  "accessibility_fix": TesseraIcons.AccessibilityIcon,
  "doc_compare": TesseraIcons.GitCompareIcon,
  "js_enabled": TesseraIcons.ToggleLeftIcon,
  "js_run": TesseraIcons.FileCodeIcon,
  "measure_scale": TesseraIcons.RulerIcon,
  "ocr_status": TesseraIcons.ScanTextIcon,
  "redact_mark": TesseraIcons.EyeOffIcon,
})
