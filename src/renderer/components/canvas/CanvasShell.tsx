/**
 * The figcraft editor iframe for one mount. Owns the bridge for that mount:
 * the bridge starts listening first, then `src` is set programmatically so
 * the URL (it carries the `ct` token) never appears in the DOM attributes or
 * in a log. Remounting (a new `mountSeq`) is a new component instance.
 *
 * No `sandbox`: the editor needs its own origin's storage and the parent
 * `postMessage` channel, both of which a sandbox would cut.
 */

import { useEffect, useRef } from 'react'
import { buildCanvasEmbedUrl, canvasOriginOf, createCanvasBridge } from '@/lib/canvas-bridge'
import { type CanvasMount, useCanvasStore } from '@/stores/canvas-store'

interface CanvasShellProps {
  projectId: string
  mount: CanvasMount
  title: string
}

export function CanvasShell({ projectId, mount, title }: CanvasShellProps): React.JSX.Element {
  const iframeRef = useRef<HTMLIFrameElement>(null)

  useEffect(() => {
    const iframe = iframeRef.current
    if (!iframe) return
    const { mountSeq, info } = mount
    const store = useCanvasStore.getState()
    const bridge = createCanvasBridge({
      iframe,
      canvasOrigin: canvasOriginOf(info.baseUrl),
      mountSeq,
      onEvent: (event) => {
        useCanvasStore.getState().handleBridgeEvent(projectId, mountSeq, event)
      }
    })
    store.attachBridge(projectId, mountSeq, bridge)
    iframe.src = buildCanvasEmbedUrl(info, window.location.origin, mountSeq)
    return () => {
      useCanvasStore.getState().detachBridge(projectId, bridge)
    }
  }, [projectId, mount])

  return (
    <iframe
      ref={iframeRef}
      title={title}
      allow="clipboard-read; clipboard-write"
      className="block h-full w-full border-0"
      data-canvas-mount-seq={mount.mountSeq}
    />
  )
}
