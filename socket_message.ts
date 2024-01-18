export interface SocketMessage {
	prefix: string
	context: string | null
	body: unknown | null
}

const indexWithFallback = (number: number, fallback: number) => number == -1 ? fallback : number

export function parseSocketMessage(text: string): SocketMessage {
	const firstParen = indexWithFallback(text.indexOf('('), text.length)
	const secondParen = indexWithFallback(text.indexOf(')', firstParen), text.length)
	const prefix = text.slice(0, firstParen)
	const context = text.slice(firstParen + 1, secondParen) || null
	const bodyString = text.slice(secondParen + 1).trim() || null
	const body = bodyString ? JSON.parse(bodyString) : null

	return { prefix, context, body }
}

export function stringifySocketMessage(message: SocketMessage): string {
	return `${message.prefix}(${message.context ?? ''}) ${JSON.stringify(message.body)}`
}
