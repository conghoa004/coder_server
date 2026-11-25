
import mysql.connector
from mysql.connector import Error
import sys

# ==========================================
# MySQL Configuration
# ==========================================
MYSQL_CONFIG = {
    "host": "127.0.0.1",
    "port": "<your_port>",
    "user": "<your_user>",
    "password": "<your_password>",
    "connection_timeout": 10
}


def test_mysql_connection():
    connection = None
    cursor = None

    print("=" * 50)
    print("         MYSQL CONNECTION TEST")
    print("=" * 50)

    try:
        # Connect to MySQL
        print("[INFO] Connecting to MySQL...")

        connection = mysql.connector.connect(**MYSQL_CONFIG)

        if not connection.is_connected():
            raise Error("Unable to establish MySQL connection.")

        print("[SUCCESS] MySQL connection established!")

        # Server information
        print(f"[INFO] MySQL Version: {connection.server_info}")

        # Create cursor
        cursor = connection.cursor()

        # Database information
        cursor.execute(
            "SELECT VERSION(), DATABASE(), USER(), NOW()"
        )

        result = cursor.fetchone()

        print("\n[INFO] Database Information")
        print(f"  Version  : {result[0]}")
        print(f"  Database : {result[1]}")
        print(f"  User     : {result[2]}")
        print(f"  Time     : {result[3]}")

        # Test SQL query
        cursor.execute("SELECT 1")
        result = cursor.fetchone()

        if result and result[0] == 1:
            print("\n[SUCCESS] SQL Query Test: PASSED")
        else:
            raise Error("SQL query test failed.")

    except Error as e:
        print("\n[ERROR] MySQL Connection Failed!")
        print(f"[DETAIL] {e}")
        sys.exit(1)

    finally:
        if cursor is not None:
            cursor.close()

        if connection is not None and connection.is_connected():
            connection.close()
            print("\n[INFO] Connection closed.")

        print("=" * 50)


if __name__ == "__main__":
    test_mysql_connection()