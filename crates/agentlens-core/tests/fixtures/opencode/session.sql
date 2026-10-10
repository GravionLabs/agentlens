-- Times are milliseconds; T0 = 1780000000000 (2026-05-28T20:26:40Z).
INSERT INTO session VALUES
  ('ses_root', 'p1', NULL, '/work/demo', 'Demo', '{"id":"model-a","providerID":"prov"}', 1780000000000, 1780000100000),
  ('ses_sub', 'p1', 'ses_root', '/work/demo', 'Subagent', '{"id":"model-b","providerID":"prov"}', 1780000030000, 1780000040000),
  ('ses_old', 'p1', NULL, '/work/old', 'Old', NULL, 1700000000000, 1700000001000);

-- Root session: user turn, assistant with steps (skill + failed read + bash), user turn,
-- assistant without step records, compaction.
INSERT INTO message VALUES
  ('m1', 'ses_root', 1780000000000, 1780000000000, '{"role":"user","time":{"created":1780000000000},"agent":"build","model":{"providerID":"prov","modelID":"model-a"}}'),
  ('m2', 'ses_root', 1780000001000, 1780000020000, '{"role":"assistant","modelID":"model-a","providerID":"prov","cost":0.03,"tokens":{"total":1700,"input":300,"output":100,"reasoning":0,"cache":{"read":1000,"write":300}}}'),
  ('m3', 'ses_root', 1780000060000, 1780000060000, '{"role":"user","time":{"created":1780000060000},"agent":"build","model":{"providerID":"prov","modelID":"model-a"}}'),
  ('m4', 'ses_root', 1780000061000, 1780000070000, '{"role":"assistant","modelID":"model-a","providerID":"prov","cost":0.01,"tokens":{"total":600,"input":100,"output":50,"reasoning":50,"cache":{"read":400,"write":0}}}');

INSERT INTO part VALUES
  ('p01', 'm1', 'ses_root', 1780000000000, 1780000000000, '{"type":"text","text":"synthetic prompt"}'),
  ('p02', 'm2', 'ses_root', 1780000002000, 1780000002000, '{"type":"tool","tool":"skill","callID":"c1","state":{"status":"completed","input":{"name":"demo-skill"},"output":"0123456789012345678901234567890123456789","metadata":{"name":"demo-skill","dir":"/home/u/.claude/skills/demo-skill"},"time":{"start":1780000002000,"end":1780000002500}}}'),
  ('p03', 'm2', 'ses_root', 1780000004000, 1780000004000, '{"type":"step-finish","reason":"tool-calls","tokens":{"total":1000,"input":100,"output":20,"reasoning":0,"cache":{"read":500,"write":300}},"cost":0.01}'),
  ('p04', 'm2', 'ses_root', 1780000005000, 1780000005000, '{"type":"tool","tool":"read","callID":"c2","state":{"status":"error","input":{"filePath":"/work/demo/missing.txt"},"output":"not found","time":{"start":1780000005000,"end":1780000005100}}}'),
  ('p05', 'm2', 'ses_root', 1780000006000, 1780000006000, '{"type":"tool","tool":"bash","callID":"c3","state":{"status":"completed","input":{"command":"curl -H secret https://example.test"},"output":"ok","time":{"start":1780000006000,"end":1780000007000}}}'),
  ('p06', 'm2', 'ses_root', 1780000020000, 1780000020000, '{"type":"step-finish","reason":"stop","tokens":{"total":700,"input":200,"output":80,"reasoning":0,"cache":{"read":500,"write":0}},"cost":0.02}'),
  ('p07', 'm4', 'ses_root', 1780000080000, 1780000080000, '{"type":"compaction","auto":true,"overflow":false}'),
  ('p08', 'm4', 'ses_root', 1780000081000, 1780000081000, 'not json');

-- Subagent session started from the root session.
INSERT INTO message VALUES
  ('s1', 'ses_sub', 1780000031000, 1780000040000, '{"role":"assistant","modelID":"model-b","providerID":"prov","cost":0.005,"tokens":{"total":250,"input":50,"output":20,"reasoning":0,"cache":{"read":180,"write":0}}}');
INSERT INTO part VALUES
  ('q1', 's1', 'ses_sub', 1780000039000, 1780000039000, '{"type":"step-finish","reason":"stop","tokens":{"total":250,"input":50,"output":20,"reasoning":0,"cache":{"read":180,"write":0}},"cost":0.005}');
