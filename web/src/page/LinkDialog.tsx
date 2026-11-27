import { Button, Dialog, useKitEditor } from '@kition/sdk/editor'

/** Asks before a web address in the document opens (a link's click): Open sends it to the host, which opens it in the browser. */
export function LinkDialog({ url, onOpen, onClose }: { url: string; onOpen: () => void; onClose: () => void }) {
  const { t } = useKitEditor()
  return (
    <Dialog
      title={t('linkOpenTitle')}
      onClose={onClose}
      testId="pdf-link-dialog"
      actions={
        <>
          <Button onClick={onClose} data-testid="pdf-link-cancel">{t('cancel')}</Button>
          <Button variant="primary" onClick={onOpen} data-testid="pdf-link-open">{t('linkOpen')}</Button>
        </>
      }
    >
      <p>{t('linkOpenText')}</p>
      <p className="pdf-link-url" data-testid="pdf-link-url">{url}</p>
    </Dialog>
  )
}
