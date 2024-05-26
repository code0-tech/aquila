import tech.code0.communication.RabbitConnection;
import tech.code0.configuration.AquilaConfiguration;
import tech.code0.data.RedisConnection;

import java.util.logging.Logger;

public final Logger logger = Logger.getLogger("AquilaServer");

void main() {
    this.logger.info("Starting Aquila Server");
    final var aquilaConfiguration = new AquilaConfiguration();
    final var redisConnection = new RedisConnection(aquilaConfiguration);
    final var rabbitConnection = new RabbitConnection(aquilaConfiguration);
}