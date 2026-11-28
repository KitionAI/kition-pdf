/**
 * Apply redactions asks first (stage 10): the text, images, drawings, comments and form fields
 * under the marks are removed and boxes drawn in their place; undo brings them back until the file
 * is saved, and the saved file no longer has them. Cancel leaves the marks as they are.
 */
import { Button, Dialog, useKitEditor } from '@kition/sdk/editor'

export function ApplyRedactions({ marks, onApply, onClose }: { marks: number; onApply: () => void; onClose: () => void }) {
  const { t } = useKitEditor()
  return (
    <Dialog
      title={t('applyRedactionsTitle')}
      onClose={onClose}
      testId="pdf-apply-redactions"
      actions={
        <>
          <Button onClick={onClose} data-testid="pdf-apply-redactions-cancel">{t('cancel')}</Button>
          <Button variant="danger" onClick={onApply} data-testid="pdf-apply-redactions-apply">{t('applyRedactionsRun')}</Button>
        </>
      }
    >
      <p data-testid="pdf-apply-redactions-text">{marks === 1 ? t('applyRedactionsOne') : t('applyRedactionsMany', { count: String(marks) })}</p>
    </Dialog>
  )
}
