import { describe, it, expect } from 'vitest';
import { parseOpencodeJsonLine } from './parser';

describe('parseOpencodeJsonLine', () => {
  it('should parse step_start event', () => {
    const line = JSON.stringify({
      type: 'step_start',
      timestamp: 1234567890,
      part: { type: 'step-start', id: 'prt_1', messageID: 'msg_1' },
    });
    const result = parseOpencodeJsonLine(line);
    expect(result).not.toBeNull();
    expect(result?.type).toBe('step_start');
    expect(result?.step).toBe('思考中');
  });

  it('should parse text event', () => {
    const line = JSON.stringify({
      type: 'text',
      timestamp: 1234567890,
      part: { type: 'text', text: 'Hello world', id: 'prt_2' },
    });
    const result = parseOpencodeJsonLine(line);
    expect(result).not.toBeNull();
    expect(result?.type).toBe('text');
    expect(result?.text).toBe('Hello world');
  });

  it('should parse step_finish event', () => {
    const line = JSON.stringify({
      type: 'step_finish',
      timestamp: 1234567890,
      part: { type: 'step-finish', reason: 'stop' },
    });
    const result = parseOpencodeJsonLine(line);
    expect(result).not.toBeNull();
    expect(result?.type).toBe('step_complete');
  });

  it('should parse actual opencode CLI output', () => {
    const lines = [
      '{"type":"step_start","timestamp":1780366997815,"sessionID":"ses_1","part":{"id":"prt_1","messageID":"msg_1","sessionID":"ses_1","snapshot":"abc","type":"step-start"}}',
      '{"type":"text","timestamp":1780366998017,"sessionID":"ses_1","part":{"id":"prt_2","messageID":"msg_1","sessionID":"ses_1","type":"text","text":"hello","time":{"start":1,"end":2}}}',
      '{"type":"step_finish","timestamp":1780366998043,"sessionID":"ses_1","part":{"id":"prt_3","reason":"stop","snapshot":"abc","messageID":"msg_1","sessionID":"ses_1","type":"step-finish","tokens":{"total":10779,"input":10774,"output":5,"reasoning":0,"cache":{"write":0,"read":0}},"cost":0}}',
    ];

    const events = lines.map((l) => parseOpencodeJsonLine(l)).filter(Boolean);
    expect(events).toHaveLength(3);
    expect(events[0]?.type).toBe('step_start');
    expect(events[1]?.type).toBe('text');
    expect(events[1]?.text).toBe('hello');
    expect(events[2]?.type).toBe('step_complete');
  });

  it('should return null for invalid JSON', () => {
    expect(parseOpencodeJsonLine('not json')).toBeNull();
    expect(parseOpencodeJsonLine('')).toBeNull();
    expect(parseOpencodeJsonLine('   ')).toBeNull();
  });

  it('should return null for unknown type', () => {
    const line = JSON.stringify({ type: 'unknown_event', data: {} });
    expect(parseOpencodeJsonLine(line)).toBeNull();
  });
});
