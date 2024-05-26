import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import tech.code0.communication.RabbitConnection;
import tech.code0.configuration.AquilaConfiguration;
import tech.code0.data.RedisConnection;

public final Logger logger = LoggerFactory.getLogger("AquilaServer");

void main() {
    this.logger.info("Starting Aquila Server");
    final var aquilaConfiguration = new AquilaConfiguration();
    final var redisConnection = new RedisConnection(aquilaConfiguration);
    final var rabbitConnection = new RabbitConnection(aquilaConfiguration);
}