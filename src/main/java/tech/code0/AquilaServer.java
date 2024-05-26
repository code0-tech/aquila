import tech.code0.communication.RabbitConnection;
import tech.code0.configuration.AquilaConfiguration;
import tech.code0.data.RedisConnection;

import static tech.code0.util.AquilaLogger.LOGGER;

void main() {

    LOGGER.info("Starting Aquila Server");
    final var aquilaConfiguration = new AquilaConfiguration();
    final var redisConnection = new RedisConnection(aquilaConfiguration);
    final var rabbitConnection = new RabbitConnection(aquilaConfiguration);
}