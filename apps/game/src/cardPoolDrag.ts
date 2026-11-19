import type { ObjectDirective } from 'vue'

const cleanupByElement = new WeakMap<HTMLElement, () => void>()
const dragThreshold = 5

export const vCardPoolDrag: ObjectDirective<HTMLElement> = {
  mounted(element) {
    const listeners = new AbortController()
    let gesture: {
      pointerId: number
      startX: number
      startY: number
      startScroll: number
      dragging: boolean
      listeners: AbortController
    } | undefined
    let suppressClick = false

    const stop = () => {
      const current = gesture
      gesture = undefined
      if (!current) return
      current.listeners.abort()
      element.removeAttribute('data-card-dragging')
      if (element.hasPointerCapture(current.pointerId)) element.releasePointerCapture(current.pointerId)
    }
    const move = (event: PointerEvent) => {
      if (!gesture || gesture.pointerId !== event.pointerId) return
      const distance = event.clientX - gesture.startX
      if (!gesture.dragging) {
        if (Math.abs(distance) < dragThreshold || Math.abs(distance) < Math.abs(event.clientY - gesture.startY)) return
        element.setPointerCapture(event.pointerId)
        gesture.dragging = true
        suppressClick = true
        element.setAttribute('data-card-dragging', '')
      }
      event.preventDefault()
      element.scrollLeft = gesture.startScroll - distance
    }
    const end = (event: PointerEvent) => {
      if (gesture?.pointerId === event.pointerId) stop()
    }

    element.addEventListener('pointerdown', (event) => {
      if (gesture) return
      suppressClick = false
      if (!event.isPrimary || event.button !== 0 || event.pointerType === 'touch' ||
        element.scrollWidth <= element.clientWidth) return

      const pointerListeners = new AbortController()
      gesture = {
        pointerId: event.pointerId,
        startX: event.clientX,
        startY: event.clientY,
        startScroll: element.scrollLeft,
        dragging: false,
        listeners: pointerListeners,
      }
      const options = { capture: true, signal: pointerListeners.signal }
      window.addEventListener('pointermove', move, options)
      window.addEventListener('pointerup', end, options)
      window.addEventListener('pointercancel', end, options)
      window.addEventListener('blur', stop, options)
    }, { signal: listeners.signal })
    element.addEventListener('lostpointercapture', (event) => {
      if (event.target === element) end(event)
    }, { signal: listeners.signal })
    element.addEventListener('dragstart', (event) => {
      if (gesture) event.preventDefault()
    }, { signal: listeners.signal })
    element.addEventListener('click', (event) => {
      if (!suppressClick || event.detail === 0) return
      suppressClick = false
      event.preventDefault()
      event.stopImmediatePropagation()
    }, { capture: true, signal: listeners.signal })

    cleanupByElement.set(element, () => {
      stop()
      listeners.abort()
    })
  },
  beforeUnmount(element) {
    cleanupByElement.get(element)?.()
    cleanupByElement.delete(element)
  },
}
