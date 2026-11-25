/**
 * Tessera icons for the PDF engine's commands that @kition/sdk/editor's rules cannot read (they draw
 * the neutral command mark otherwise): its own vocabulary, by command id. Registered once, when the
 * editor loads (`main.tsx`); ribbons and menus built from engine commands draw them.
 */
import { TesseraIcons, registerCommandIcons } from '@kition/sdk/editor'


registerCommandIcons({
  "accessibility.fix": TesseraIcons.AccessibilityIcon,
  "file.compare": TesseraIcons.GitCompareIcon,
  "script.enabled": TesseraIcons.ToggleLeftIcon,
  "script.run": TesseraIcons.FileCodeIcon,
  "measure.setScale": TesseraIcons.RulerIcon,
  "ocr.status": TesseraIcons.ScanTextIcon,
  "redaction.mark": TesseraIcons.EyeOffIcon,
})
