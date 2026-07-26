import { useEffect, useRef } from "react";

export function DeleteModal({ titleText, bodyText, pathText, twoStage, stage, deleting, onCancel, onConfirm }: { titleText: string; bodyText: string; pathText?: string; twoStage: boolean; stage: 'warn' | 'confirm'; deleting: boolean; onCancel: () => void; onConfirm: () => void; }) {
  const cancelRef = useRef<HTMLButtonElement>(null);
  useEffect(() => { cancelRef.current?.focus(); }, [stage]);
  useEffect(() => { function h(e: KeyboardEvent) { if (e.key === 'Escape') { e.preventDefault(); onCancel(); } } window.addEventListener('keydown', h); return () => window.removeEventListener('keydown', h); }, [onCancel]);
  const isSecond = twoStage && stage === 'confirm';
  return (
    <div className="modal-overlay modal-overlay-danger" role="presentation" onMouseDown={onCancel}>
      <div className="modal-dialog modal-danger" role="alertdialog" aria-modal="true" aria-labelledby="del-title" aria-describedby="del-body" onMouseDown={(e) => e.stopPropagation()}>
        <div className="modal-icon" aria-hidden="true">⚠</div>
        {isSecond ? (<><h3 id="del-title">二次确认</h3><div id="del-body" className="modal-body"><p>请再次确认：确定要<strong>永久删除</strong>「<strong>{titleText}</strong>」吗？删除后<strong>无法恢复</strong>。</p></div></>) : (<><h3 id="del-title">{twoStage ? '永久删除项目？' : '删除结构？'}</h3><div id="del-body" className="modal-body"><p>{bodyText}</p>{pathText ? <p className="modal-path mono">{pathText}</p> : null}</div></>)}
        <div className="modal-actions">
          <button type="button" className="modal-cancel" ref={cancelRef} onClick={onCancel} disabled={deleting}>取消</button>
          <button type="button" className="modal-delete" onClick={onConfirm} disabled={deleting}>{isSecond ? (deleting ? '删除中…' : '确认删除') : (twoStage ? '删除' : (deleting ? '删除中…' : '确认删除'))}</button>
        </div>
      </div>
    </div>
  );
}

