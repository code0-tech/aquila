package tech.code0.communication;

import com.rabbitmq.client.Connection;
import com.rabbitmq.client.ConnectionFactory;
import lombok.Getter;
import tech.code0.configuration.AquilaConfiguration;

import static tech.code0.util.AquilaLogger.LOGGER;

@Getter
public class RabbitConnection {

    private final Connection connection;

    public RabbitConnection(AquilaConfiguration aquilaConfiguration) {
        LOGGER.info("Initializing Rabbit connection");
        this.connection = createConnection(aquilaConfiguration);
    }

    private Connection createConnection(AquilaConfiguration aquilaConfiguration) {
        final var connectionFactory = new ConnectionFactory();
        connectionFactory.setHost(aquilaConfiguration.getRabbitMQHost());
        connectionFactory.setPort(aquilaConfiguration.getRabbitMQPort());

        try (final var connection = connectionFactory.newConnection()) {
            LOGGER.info("Connected to RabbitMQ");
            return connection;
        } catch (Exception exception) {
            LOGGER.warning("Connection to RabbitMQ failed: " + exception.getMessage());
            throw new RuntimeException(exception);
        }
    }

    public void close() {
        try {
            LOGGER.warning("Closing RabbitMQ connection");
            this.connection.close();
        } catch (Exception exception) {
            LOGGER.severe("Failed to close RabbitMQ connection: " + exception.getMessage());
        }
    }
}