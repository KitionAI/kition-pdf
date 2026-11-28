/**
 * The Forms tab's Fill & Sign group (stage 10, the walkthrough's S4): on a page with no form
 * fields, Text puts typed text where the page is clicked and Check and Cross put a mark centred on
 * the click, each one `fillSign.add` (one undo step). The chosen one stays on for the next click
 * until it is clicked again, Escape is pressed or a tool on the rail is chosen.
 */
import { RibbonButton, RibbonGroup, RibbonStack, TesseraIcons, useKitEditor } from '@kition/sdk/editor'

export type FillMark = 'text' | 'check' | 'cross'

const Type = TesseraIcons.TypeIcon
const Check = TesseraIcons.CheckIcon
const Cross = TesseraIcons.XIcon
const Signature = TesseraIcons.SignatureIcon

export function FillSignGroup({ mark, editable, onMark }: { mark: FillMark | null; editable: boolean; onMark: (mark: FillMark | null) => void }) {
  const { t, enabled } = useKitEditor()
  const off = !editable || !enabled('fillSign.add')
  const button = (id: FillMark, icon: typeof Type, label: string, large = false) => (
    <RibbonButton icon={icon} size={large ? 'large' : undefined} label={label} pressed={mark === id} disabled={off} onClick={() => onMark(mark === id ? null : id)} data-testid={`pdf-fill-${id}`} primary />
  )
  return (
    <RibbonGroup label={t('fillSign')} icon={Signature}>
      {button('text', Type, t('fillText'), true)}
      <RibbonStack>
        {button('check', Check, t('fillCheck'))}
        {button('cross', Cross, t('fillCross'))}
      </RibbonStack>
    </RibbonGroup>
  )
}
