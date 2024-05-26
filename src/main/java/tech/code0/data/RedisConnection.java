package tech.code0.data;

import io.lettuce.core.RedisClient;
import io.lettuce.core.api.StatefulRedisConnection;
import lombok.Getter;
import tech.code0.configuration.AquilaConfiguration;

import static tech.code0.util.AquilaLogger.LOGGER;

@Getter
public class RedisConnection {

    private final StatefulRedisConnection<String, String> connection;
    private final String connectionString;
    private final RedisClient client;

    public RedisConnection(AquilaConfiguration configuration) {
        LOGGER.info("Initializing Redis connection");
        this.connectionString = "redis://:flows@" + configuration.getRedisHost() + ":" + configuration.getRedisPort();
        this.client = RedisClient.create(connectionString);
        this.connection = client.connect();
        LOGGER.info("Connected to Redis");
    }

    public void shutdown() {
        LOGGER.warning("Shutting down RedisConnection");
        this.connection.close();
        this.client.shutdown();
    }

}
