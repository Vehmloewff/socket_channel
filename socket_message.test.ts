import { assertEquals } from 'asserts'
import { parseSocketMessage } from './socket_message.ts'

Deno.test('parseSocketMessage', () => {
	assertEquals(parseSocketMessage('prefix(context) []'), { prefix: 'prefix', context: 'context', body: [] })
	assertEquals(parseSocketMessage('  prefix  (  con text     ) [ "foo"   ]     '), {
		prefix: '  prefix  ',
		context: '  con text     ',
		body: ['foo'],
	})
	assertEquals(parseSocketMessage('prefix [ "not", "body"]'), { prefix: 'prefix [ "not", "body"]', body: null, context: null })
	assertEquals(parseSocketMessage('prefix() ["foo"]'), { prefix: 'prefix', context: null, body: ['foo'] })
})
