import { parseSocketMessage, stringifySocketMessage } from './socket_message.ts'

export type ErrorHandler = (message: string) => void

export class SocketClient {
	#address: URL
	#socketPromise: Promise<WebSocket>
	#fatalErrorHandler: ErrorHandler | null = null
	#modelListeners = new Set<(pin: string, model: unknown) => void>()

	constructor(address: URL) {
		this.#address = address

		this.#socketPromise = this.#connect()
	}

	/** Send an event. Promise resolves when a new model has been received which contains the computed result of this event */
	async send(event: unknown): Promise<void> {
		const pin = await this.#internalPing(event)

		await new Promise<void>((resolve) => {
			const listener = (respondedPin: string) => {
				if (respondedPin !== pin) return

				this.#modelListeners.delete(listener)
				resolve()
			}

			this.#modelListeners.add(listener)
		})
	}

	/** Send an event. Promise resolves as soon as event is sent, without waiting for an updated model to be sent back */
	async ping(event: unknown): Promise<void> {
		await this.#internalPing(event)
	}

	/** Subscribe to model updates. Returns a function to unsubscribe */
	subscribe(fn: (model: unknown) => void): () => void {
		const listener = (_: string, model: unknown) => fn(model)
		this.#modelListeners.add(listener)

		return () => this.#modelListeners.delete(listener)
	}

	/** Handle all errors from the socket. The socket will normally try to reconnect, but sometimes will still receive an internal error */
	setErrorHandler(fn: ErrorHandler): void {
		this.#fatalErrorHandler = fn
	}

	async #internalPing(event: unknown): Promise<string> {
		const socket = await this.#socketPromise
		const pin = crypto.randomUUID()

		if (socket.readyState !== socket.OPEN) {
			this.#socketPromise = this.#connect()
			return this.#internalPing(event)
		}

		socket.send(stringifySocketMessage({ body: event, context: pin, prefix: 'event' }))

		return pin
	}

	#handleError(message: string) {
		if (this.#fatalErrorHandler) return this.#fatalErrorHandler(message)

		console.error(message)
	}

	#handleMessage(data: string) {
		const message = parseSocketMessage(data)
		if (message.prefix !== 'model') return

		// If there is no message context, we want to log the error and continue, because missing one message isn't the end of the world
		if (!message.context) return this.#handleError('Expected to receive a pin from server along with model')

		for (const fn of this.#modelListeners) {
			fn(message.context, message.body)
		}
	}

	#socketOwnerId: string | null = null
	async #connect() {
		const socket = new WebSocket(this.#address)
		const ownerId = crypto.randomUUID()

		socket.onclose = ({ reason }) => {
			if (this.#socketOwnerId !== ownerId) return
			if (reason) return this.#handleError(`Socket failed: ${reason}`)

			this.#socketPromise = new Promise<WebSocket>((resolve, reject) => {
				setTimeout(() => this.#connect().then(resolve).catch(reject))
			})
		}

		socket.onmessage = ({ data }) => {
			if (typeof data !== 'string') return

			this.#handleMessage(data)
		}

		await new Promise<void>((resolve) => {
			socket.onopen = () => {
				resolve()
			}
		})

		return socket
	}
}
