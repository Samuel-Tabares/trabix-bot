-- El simulador se removio en v1.8.0; estas tablas quedaron huerfanas desde
-- entonces (cero referencias en src/). 005 se mantiene intacta (append-only).
DROP TABLE IF EXISTS simulator_media;
DROP TABLE IF EXISTS simulator_messages;
DROP TABLE IF EXISTS simulator_sessions;
